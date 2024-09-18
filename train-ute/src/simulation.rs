use std::io::Write;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Instant;

use kdam::{TqdmIterator, TqdmParallelIterator};
use rand::prelude::*;
use raptor::network::{GlobalTripIndex, PathfindingCost, StopIndex, Timestamp};
use raptor::{csa_query, raptor_query, Network};
use rayon::prelude::*;

pub type AgentCount = u16;
pub type PopulationCount = i32;
pub type PopulationCountAtomic = AtomicI32;
pub type CrowdingCost = PathfindingCost;

pub trait SimulationParams {
    fn max_train_capacity(&self) -> AgentCount;
    fn cost_fn(&self, count: PopulationCount) -> CrowdingCost;
    // Called by the simulation to report progress (0-1).
    fn progress_callback(&self, percent: f32);
}

// Simulation notes:
// When we get the O-D data, we can run journey planning for each OD and apply the passenger counts to the relevant trips.
// once this is run once, we update the journey planning weights based on the crowding and run again.
// This is like the 'El Farol Bar' problem.
// Matsim-like replanning for a proportion of the population might also be viable.

// This default simulation parameter implementation uses a simple exponential crowding cost function, and can report progress.
pub struct DefaultSimulationParams<C: Fn(f32) = fn(f32)> {
    pub max_train_capacity: AgentCount,
    progress_callback: Option<C>,
}

impl<C: Fn(f32)> DefaultSimulationParams<C> {
    pub const fn new(max_train_capacity: AgentCount, progress_callback: Option<C>) -> Self {
        let result = Self {
            max_train_capacity,
            progress_callback,
        };
        result
    }
    fn f(x: CrowdingCost) -> CrowdingCost {
        const B: CrowdingCost = 5.;
        let bx = B * x;
        let ebx = bx.exp();
        (ebx - 1.) / (B.exp() - 1.)
    }
}

impl<C: Fn(f32)> SimulationParams for DefaultSimulationParams<C> {
    fn max_train_capacity(&self) -> AgentCount {
        self.max_train_capacity
    }

    fn cost_fn(&self, count: PopulationCount) -> CrowdingCost {
        debug_assert!(count >= 0, "Negative population count");
        let proportion = count as CrowdingCost / self.max_train_capacity() as CrowdingCost;
        Self::f(proportion)
    }

    fn progress_callback(&self, percent: f32) {
        self.progress_callback.as_ref().map(|f| f(percent));
    }
}
pub struct SimulationStep {
    pub departure_time: Timestamp,
    pub origin_stop: StopIndex,
    pub dest_stop: StopIndex,
    pub count: AgentCount,
}

pub struct AgentJourney {
    pub agent_id: u32,
    pub origin_stop: StopIndex,
    pub origin_trip: GlobalTripIndex,
    pub dest_stop: StopIndex,
    pub dest_trip: GlobalTripIndex,
    pub count: AgentCount,
    pub duration: Timestamp,
    pub crowding_cost: CrowdingCost,
    pub num_transfers: u8,
}

pub struct SimulationResult {
    pub population_count: Vec<PopulationCount>,
    pub agent_journeys: Vec<AgentJourney>,
}

pub fn gen_simulation_steps(network: &Network, number: Option<usize>, seed: Option<u64>) -> Vec<SimulationStep> {
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
        simulation_steps.push(SimulationStep {
            departure_time: start_time,
            origin_stop: rng.gen_range(0..num_stops),
            dest_stop: rng.gen_range(0..num_stops),
            count: rng.gen_range(1..=10),
        });
    }
    simulation_steps
}

// Const generic parameter P switched between normal (false) and prefix-sum (true) simulation.
pub fn run_simulation<T: SimulationParams, const P: bool>(network: &Network, simulation_steps: &[SimulationStep], params: &T) -> SimulationResult {
    // Agent counts need to be stored per trip stop, and signed so they can be temporarily negative.

    // Initialise agent counts to zero. To allow parallelism, we use an atomic type.
    let mut trip_stops_pop = Vec::new();
    trip_stops_pop.resize_with(network.stop_times.len(), PopulationCountAtomic::default);

    let mut trip_stops_cost = vec![0 as CrowdingCost; network.stop_times.len()];
    params.progress_callback(0.);

    let agent_journeys = simulation_steps.par_iter().tqdm().enumerate().map(|(i, sim_step)| {
        if sim_step.count == 0 {
            return None;
        }

        let journey = if false {
            csa_query(network, sim_step.origin_stop, sim_step.departure_time, sim_step.dest_stop)
            //mc_csa_query(network, journey.start_stop, journey.start_time, journey.end_stop, &trip_stops_cost)
        } else {
            raptor_query(network, sim_step.origin_stop, sim_step.departure_time, sim_step.dest_stop)
            //mc_raptor_query(network, journey.start_stop, journey.start_time, journey.end_stop, &trip_stops_cost)
        };

        if journey.legs.is_empty() {
            return None;
        }

        let mut origin_trip = None;
        let mut dest_trip = None;

        for (i, leg) in journey.legs.iter().enumerate() {
            // TODO: allow looking up by GlobalTripIndex.
            let route = &network.routes[leg.trip.route_idx as usize];
            let trip = &trip_stops_pop[route.get_trip_range(leg.trip.trip_order as usize)];

            if i == 0 {
                origin_trip = Some(leg.trip);
            }
            if i == journey.legs.len() - 1 {
                dest_trip = Some(leg.trip);
            }

            let count = sim_step.count as PopulationCount;
            let boarded_stop_order = leg.boarded_stop_order as usize;
            let arrival_stop_order = leg.arrival_stop_order as usize;
            if P {
                // Add one agent to this span of trip stops.
                trip[boarded_stop_order].fetch_add(count, Ordering::SeqCst);
                // Remove agent at stop (for inclusive-exclusive range).
                trip[arrival_stop_order].fetch_sub(count, Ordering::SeqCst);
                assert!(boarded_stop_order < arrival_stop_order, "{boarded_stop_order} < {arrival_stop_order}")
            } else {
                // Iterate over all stops in the trip, adding the agent count.
                for i in boarded_stop_order..arrival_stop_order {
                    trip[i].fetch_add(count, Ordering::SeqCst);
                }
            }
        }

        let origin_trip = origin_trip.unwrap();
        let dest_trip = dest_trip.unwrap_or(origin_trip);

        Some(AgentJourney {
            agent_id: i as u32,
            origin_stop: sim_step.origin_stop,
            origin_trip,
            dest_stop: sim_step.dest_stop,
            dest_trip,
            count: sim_step.count,
            duration: journey.duration,
            crowding_cost: 0.,
            num_transfers: (journey.legs.len() - 1) as u8,
        })
    }).filter_map(std::convert::identity).collect::<Vec<_>>();

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
        population_count: trip_stops_pop,
        agent_journeys,
    }
}

// Runs a benchmark and outputs to a csv file.
pub fn _simulation_prefix_benchmark<T: SimulationParams>(network: &Network, params: &T, file: &str) -> std::io::Result<()> {
    let mut output = Vec::new();
    // CSV header.
    writeln!(&mut output, "num_steps,with_prefix,without_prefix,percent_difference")?;
    for i in (1..18).tqdm() {
        let num_steps = 1 << i;
        let simulation_steps = gen_simulation_steps(&network, Some(num_steps), Some(0));

        let simulation_start = Instant::now();
        let mut simulation_result_1 = SimulationResult { population_count: Vec::new(), agent_journeys: Vec::new() };
        for _ in (0..5).tqdm() {
            simulation_result_1 = run_simulation::<_, true>(&network, &simulation_steps, params);
        }
        //let simulation_result_1 = run_simulation::<_, true>(&network, &simulation_steps, params);
        let simulation_duration_1 = simulation_start.elapsed() / 10;
        //println!("Simulation duration with prefix sum: {:?} to run {} steps", simulation_duration_1, simulation_steps.len());

        let simulation_start = Instant::now();
        let mut simulation_result_2 = SimulationResult { population_count: Vec::new(), agent_journeys: Vec::new() };
        for _ in (0..5).tqdm() {
            simulation_result_2 = run_simulation::<_, false>(&network, &simulation_steps, params);
        }
        let simulation_duration_2 = simulation_start.elapsed() / 10;
        //println!("Simulation duration without prefix sum: {:?} to run {} steps", simulation_duration_2, simulation_steps.len());
        let difference = (simulation_duration_1.as_nanos() as f64 / simulation_duration_2.as_nanos() as f64 - 1.) * 100.;
        //println!("% difference: {}", difference);

        assert_eq!(simulation_result_1.population_count, simulation_result_2.population_count);

        writeln!(&mut output, "{num_steps},{},{},{}", simulation_duration_1.as_micros(), simulation_duration_2.as_micros(), difference)?;
    }

    std::fs::write(file, output)?;

    Ok(())
}