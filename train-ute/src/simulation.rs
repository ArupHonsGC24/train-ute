use std::io::Write;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Instant;

use rand::prelude::*;
use rayon::prelude::*;
use tqdm::Iter;

use raptor::{Network, raptor_query};
use raptor::network::{PathfindingCost, StopIndex, Timestamp};

pub type AgentCount = u16;
pub type PopulationCount = i32;
pub type PopulationCountAtomic = AtomicI32;
pub type CrowdingCost = PathfindingCost;

pub trait SimulationParams {
    fn max_train_capacity(&self) -> AgentCount;
    fn cost_fn(&self, count: PopulationCount) -> CrowdingCost;
}

pub struct AgentJourney {
    pub start_time: Timestamp,
    pub start_stop: StopIndex,
    pub end_stop: StopIndex,
    pub count: AgentCount,
}

pub struct SimulationResult {
    pub agent_journeys: Vec<PopulationCount>,
}

pub fn gen_simulation_steps(network: &Network, number: Option<usize>, seed: Option<u64>) -> Vec<AgentJourney> {
    let mut simulation_steps = Vec::new();
    let num_stops = network.num_stops() as StopIndex;
    let mut rng = match seed {
        Some(seed) => SmallRng::seed_from_u64(seed),
        None => SmallRng::from_entropy(),
    };
        
    // New agent journey every second.
    let sim_start_time = 4 * 60 * 60; // Start at 4am.
    let sim_end_time = 24 * 60 * 60; // Final journey begins at midnight.
    let sim_length = sim_end_time - sim_start_time;
    let number = number.unwrap_or(sim_length as usize);
    let interval = sim_length as f64 / number as f64;
    for i in 0..number {
        let start_time = sim_start_time + (i as f64 * interval) as Timestamp;
        simulation_steps.push(AgentJourney {
            start_time,
            start_stop: rng.gen_range(0..num_stops),
            end_stop: rng.gen_range(0..num_stops),
            count: rng.gen_range(1..=10),
        });
    }
    simulation_steps
}

// Const generic parameter P switched between normal (false) and prefix-sum (true) simulation.
pub fn run_simulation<T: SimulationParams, const P: bool>(network: &Network, simulation_steps: &[AgentJourney], params: &T) -> SimulationResult {
    // Agent counts need to be stored per trip stop, and signed so they can be temporarily negative.

    // Initialise agent counts to zero. To allow parallelism, we use an atomic type.
    let mut trip_stops_pop = Vec::new();
    trip_stops_pop.resize_with(network.stop_times.len(), PopulationCountAtomic::default);

    let mut trip_stops_cost = vec![0 as CrowdingCost; network.stop_times.len()];
    // TODO: test just using map instead of atomics?
    simulation_steps.par_iter().for_each(|journey| {
        let query = raptor_query(network, journey.start_stop, journey.start_time, journey.end_stop, &trip_stops_cost);
        for leg in query.legs {
            let route = &network.routes[leg.route_idx as usize];
            let trip = &trip_stops_pop[route.get_trip_range(leg.trip_idx as usize)];
            let count = journey.count as PopulationCount;
            let boarded_stop_order = leg.boarded_stop_order as usize;
            let arrival_stop_order = leg.arrival_stop_order as usize;
            if P {
                // Add one agent to this span of trip stops.
                trip[boarded_stop_order].fetch_add(count, Ordering::SeqCst);
                // Remove agent at stop (for inclusive-exclusive range).
                trip[arrival_stop_order].fetch_sub(count, Ordering::SeqCst);
            } else {
                // Iterate over all stops in the trip, adding the agent count.
                for i in boarded_stop_order..arrival_stop_order {
                    trip[i].fetch_add(count, Ordering::SeqCst);
                }
            }
        }
    });

    // Copy counts from Vec<PopulationCountAtomic> to Vec<PopulationCount>.
    let mut trip_stops_pop = trip_stops_pop.iter().map(|x| x.load(Ordering::SeqCst)).collect::<Vec<PopulationCount>>();

    // Build sums of agent counts, and calculate crowding cost.
    // Note: this ends up running through the trip_pop in order, so it's cache-friendly.
    for route_idx in 0..network.routes.len() {
        let route = &network.routes[route_idx];
        for trip in 0..route.num_trips as usize {
            let trip_range = route.get_trip_range(trip);
            let trip = &mut trip_stops_pop[trip_range.clone()];
            let costs = &mut trip_stops_cost[trip_range];

            costs[0] = params.cost_fn(trip[0]);
            for i in 0..(trip.len() - 1) {
                if P {
                    trip[i + 1] += trip[i];
                }
                costs[i + 1] = params.cost_fn(trip[i + 1]);
                assert!(trip[i] >= 0);
            }
        }
    }

    SimulationResult {
        agent_journeys: trip_stops_pop,
    }
}

// Runs a benchmark and outputs to a csv file.
#[allow(dead_code)]
pub fn simulation_prefix_benchmark<T: SimulationParams>(network: &Network, params: &T, file: &str) -> std::io::Result<()> {
    let mut output = Vec::new();
    // CSV header.
    writeln!(&mut output, "num_steps,with_prefix,without_prefix,percent_difference")?;
    for i in (1..18).tqdm() {
        let num_steps = 1 << i;
        let simulation_steps = gen_simulation_steps(&network, Some(num_steps), Some(0));

        let simulation_start = Instant::now();
        let mut simulation_result_1 = SimulationResult { agent_journeys: Vec::new() };
        for _ in (0..5).tqdm() {
            simulation_result_1 = run_simulation::<_, true>(&network, &simulation_steps, params);
        }
        //let simulation_result_1 = run_simulation::<_, true>(&network, &simulation_steps, params);
        let simulation_duration_1 = simulation_start.elapsed() / 10;
        //println!("Simulation duration with prefix sum: {:?} to run {} steps", simulation_duration_1, simulation_steps.len());

        let simulation_start = Instant::now();
        let mut simulation_result_2 = SimulationResult { agent_journeys: Vec::new() };
        for _ in (0..5).tqdm() {
            simulation_result_2 = run_simulation::<_, false>(&network, &simulation_steps, params);
        }
        let simulation_duration_2 = simulation_start.elapsed() / 10;
        //println!("Simulation duration without prefix sum: {:?} to run {} steps", simulation_duration_2, simulation_steps.len());
        let difference = (simulation_duration_1.as_nanos() as f64 / simulation_duration_2.as_nanos() as f64 - 1.) * 100.;
        //println!("% difference: {}", difference);

        assert_eq!(simulation_result_1.agent_journeys, simulation_result_2.agent_journeys);

        writeln!(&mut output, "{num_steps},{},{},{}", simulation_duration_1.as_micros(), simulation_duration_2.as_micros(), difference)?;
    }
    
    std::fs::write(file, output)?;
    
    Ok(())
}