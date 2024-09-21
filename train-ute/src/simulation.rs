use std::sync::atomic::{AtomicI32, Ordering};
use itertools::izip;
#[cfg(feature = "progress_bar")]
use kdam::{Spinner, par_tqdm, tqdm};
#[cfg(feature = "progress_bar")]
use std::io::IsTerminal;
use either::Either;
use rand::prelude::*;
use raptor::journey::{JourneyError, JourneyPreferences};
use raptor::network::{GlobalTripIndex, PathfindingCost, StopIndex, Timestamp};
use raptor::Network;
use rayon::prelude::*;

pub type AgentCount = u16;
pub type PopulationCount = i32;
pub type PopulationCountAtomic = AtomicI32;
pub type CrowdingCost = PathfindingCost;

pub type SimulationProgressCallback<'a> = dyn Fn(f32, f32) + 'a;
pub trait SimulationParams {
    fn max_train_capacity(&self) -> AgentCount;
    fn cost_fn(&self, count: PopulationCount) -> CrowdingCost;
    fn get_journey_preferences(&self) -> &JourneyPreferences;
    fn get_num_rounds(&self) -> u16;
    fn get_progress_callback(&self) -> Option<&SimulationProgressCallback>;
    // Called by the simulation to report progress (0-1).
    fn run_progress_callback(&self, total_progress: f32, round_progress: f32) {
        self.get_progress_callback().as_ref().map(|f| f(total_progress, round_progress));
    }
}

// Simulation notes:
// When we get the O-D data, we can run journey planning for each OD and apply the passenger counts to the relevant trips.
// once this is run once, we update the journey planning weights based on the crowding and run again.
// This is like the 'El Farol Bar' problem.
// Matsim-like replanning for a proportion of the population might also be viable.

// This default simulation parameter implementation uses a simple exponential crowding cost function, and can report progress.
pub struct DefaultSimulationParams<'a> {
    pub max_train_capacity: AgentCount,
    pub progress_callback: Option<Box<SimulationProgressCallback<'a>>>,
    pub journey_preferences: JourneyPreferences,
    pub num_rounds: u16,
}

impl DefaultSimulationParams<'_> {
    fn f(x: CrowdingCost) -> CrowdingCost {
        const B: CrowdingCost = 5.;
        let bx = B * x;
        let ebx = bx.exp();
        (ebx - 1.) / (B.exp() - 1.)
    }
}

impl SimulationParams for DefaultSimulationParams<'_> {
    fn max_train_capacity(&self) -> AgentCount {
        self.max_train_capacity
    }

    fn cost_fn(&self, count: PopulationCount) -> CrowdingCost {
        debug_assert!(count >= 0, "Negative population count");
        let proportion = count as CrowdingCost / self.max_train_capacity() as CrowdingCost;
        Self::f(proportion)
    }

    fn get_journey_preferences(&self) -> &JourneyPreferences {
        &self.journey_preferences
    }

    fn get_num_rounds(&self) -> u16 {
        self.num_rounds
    }

    fn get_progress_callback(&self) -> Option<&SimulationProgressCallback> {
        self.progress_callback.as_ref().map(|f| f.as_ref())
    }
}

pub struct SimulationStep {
    pub departure_time: Timestamp,
    pub origin_stop: StopIndex,
    dest_stops: Vec<StopIndex>,
    counts: Vec<AgentCount>,
}

impl SimulationStep {
    pub fn new(departure_time: Timestamp, origin_stop: StopIndex) -> Self {
        Self {
            departure_time,
            origin_stop,
            dest_stops: Vec::new(),
            counts: Vec::new(),
        }
    }
    pub fn count(&self) -> AgentCount {
        self.counts.iter().sum()
    }
    pub fn push(&mut self, dest_stop: StopIndex, count: AgentCount) {
        self.dest_stops.push(dest_stop);
        self.counts.push(count);
    }
}

pub struct AgentJourney {
    pub origin_stop: StopIndex,
    pub origin_trip: GlobalTripIndex,
    pub dest_stop: StopIndex,
    pub dest_trip: GlobalTripIndex,
    pub count: AgentCount,
    pub duration: Timestamp,
    pub crowding_cost: CrowdingCost,
    pub num_transfers: u8,
}

pub struct AgentJourneyResult {
    pub sim_step_idx: u32,
    pub journey_idx: u32,
    pub result: Result<AgentJourney, JourneyError>,
}

pub struct SimulationRoundResult {
    pub population_count: Vec<PopulationCount>,
    pub crowding_cost: Vec<CrowdingCost>,
    pub agent_journeys: Vec<AgentJourneyResult>,
}

pub struct SimulationResult {
    pub population_count: Vec<PopulationCount>,
    pub round_agent_journeys: Vec<Vec<AgentJourneyResult>>,
}

impl SimulationResult {
    pub fn print_stats(&self) {
        println!("Rounds: {}", self.round_agent_journeys.len());
        println!("Agent journeys: {}", self.round_agent_journeys.last().map(|v| v.len()).unwrap_or(0));
    }
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
            dest_stops: vec![rng.gen_range(0..num_stops)],
            counts: vec![rng.gen_range(1..=10)],
        });
    }
    simulation_steps
}

fn run_simulation_round(network: &Network,
                        simulation_steps: &[SimulationStep],
                        params: &impl SimulationParams,
                        crowding_cost: Option<&[CrowdingCost]>,
                        _round_number: u16) -> SimulationRoundResult {
    
    // Initialise agent counts to zero. To allow parallelism, we use an atomic type.
    let mut trip_stops_pop = Vec::new();
    trip_stops_pop.resize_with(network.stop_times.len(), PopulationCountAtomic::default);

    let mut zero_crowding_cost = Vec::new();

    let crowding_cost = crowding_cost.unwrap_or_else(|| {
        zero_crowding_cost = vec![0 as CrowdingCost; network.stop_times.len()];
        &zero_crowding_cost
    });

    assert_eq!(trip_stops_pop.len(), crowding_cost.len());

    let journey_preferences = params.get_journey_preferences();

    let step_iterator = simulation_steps.par_iter();

    #[cfg(feature = "progress_bar")]
    let step_iterator = {
        let spin = Spinner::new(
            &["▁▂▃", "▂▃▄", "▃▄▅", "▄▅▆", "▅▆▇", "▆▇█", "▇█▇", "█▇▆", "▇▆▅", "▆▅▄", "▅▄▃", "▄▃▂", "▃▂▁"],
            30.0,
            1.0,
        );
        par_tqdm!(
            step_iterator,
            desc = "Simulation Steps", 
            position = _round_number + 1,
            animation = kdam::Animation::FillUp,
            spinner = spin,
            bar_format = "{desc}{percentage:3.0}%|{animation}|{spinner} {count}/{total} [{elapsed}<{remaining}, {rate:.2}{unit}/s{postfix}]"
        )
    };

    let agent_journeys = step_iterator
        .enumerate()
        .flat_map_iter(|(sim_step_idx, sim_step)| {
            let sim_step_idx = sim_step_idx as u32;
            if sim_step.count() == 0 {
                // Ignore zero-count agents.
                return Either::Left((0..sim_step.dest_stops.len() as u32).map(move |journey_idx| {
                    AgentJourneyResult {
                        sim_step_idx,
                        journey_idx,
                        result: Err(JourneyError::NoJourneyFound),
                    }
                }));
            }

            //let journey = raptor::raptor_query(network, sim_step.origin_stop, sim_step.departure_time, sim_step.dest_stop);
            let journeys = raptor::mc_raptor_query(network,
                                                   sim_step.origin_stop,
                                                   sim_step.departure_time,
                                                   &sim_step.dest_stops,
                                                   crowding_cost,
                                                   &journey_preferences);

            // Bind to reference so we can use in the move closure.
            let trip_stops_pop = &trip_stops_pop;
            Either::Right(
                izip!(0..journeys.len() as u32, journeys.into_iter(), &sim_step.counts, &sim_step.dest_stops)
                .map(move |(journey_idx, journey, &count, &dest_stop)| {
                    let journey = match journey {
                        Ok(journey) => journey,
                        Err(err) => return AgentJourneyResult {
                            sim_step_idx,
                            journey_idx,
                            result: Err(err),
                        },
                    };

                    if journey.legs.is_empty() {
                        // Ignore empty journeys.
                        return AgentJourneyResult {
                            sim_step_idx,
                            journey_idx,
                            result: Err(JourneyError::NoJourneyFound),
                        };
                    }

                    // Because journey.legs.len() > 0, these are guaranteed to be set in the loop;
                    let mut origin_trip = GlobalTripIndex::default();
                    let mut dest_trip = GlobalTripIndex::default();

                    for (i, leg) in journey.legs.iter().enumerate() {
                        let route = &network.routes[leg.trip.route_idx as usize];
                        let trip = &trip_stops_pop[route.get_trip_range(leg.trip.trip_order as usize)];

                        // Record first and last trip.
                        if i == 0 {
                            origin_trip = leg.trip;
                        }
                        if i == journey.legs.len() - 1 {
                            dest_trip = leg.trip;
                        }

                        let count = count as PopulationCount;
                        let boarded_stop_order = leg.boarded_stop_order as usize;
                        let arrival_stop_order = leg.arrival_stop_order as usize;
                        // Add one agent to this span of trip stops.
                        trip[boarded_stop_order].fetch_add(count, Ordering::Relaxed);
                        // Remove agent at stop (for inclusive-exclusive range).
                        trip[arrival_stop_order].fetch_sub(count, Ordering::Relaxed);

                        // Non-prefix-sum version.
                        //{
                        //    assert!(boarded_stop_order < arrival_stop_order, "{boarded_stop_order} < {arrival_stop_order}")
                        //    // Iterate over all stops in the trip, adding the agent count.
                        //    for i in boarded_stop_order..arrival_stop_order {
                        //        trip[i].fetch_add(count, Ordering::Relaxed);
                        //    }
                        //}
                    }

                    AgentJourneyResult {
                        sim_step_idx,
                        journey_idx,
                        result: Ok(AgentJourney {
                            origin_stop: sim_step.origin_stop,
                            origin_trip,
                            dest_stop,
                            dest_trip,
                            count,
                            duration: journey.duration,
                            crowding_cost: 0., // TODO: calculate crowding cost.
                            num_transfers: (journey.legs.len() - 1) as u8,
                        }),
                    }
                }))
        }).collect::<Vec<_>>();

    let mut trip_stops_cost = vec![0 as CrowdingCost; network.stop_times.len()];

    // Copy counts from Vec<PopulationCountAtomic> to Vec<PopulationCount>.
    let mut trip_stops_pop = trip_stops_pop.iter().map(|x| x.load(Ordering::Relaxed)).collect::<Vec<PopulationCount>>();

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
                // Calculate prefix sums
                trip[i + 1] += trip[i];
                // Calculate crowding cost.
                costs[i + 1] = params.cost_fn(trip[i + 1]);
                assert!(trip[i] >= 0);
            }
        }
    }

    SimulationRoundResult {
        population_count: trip_stops_pop,
        crowding_cost: trip_stops_cost,
        agent_journeys,
    }
}

// Const generic parameter P switched between normal (false) and prefix-sum (true) simulation.
pub fn run_simulation(network: &Network, simulation_steps: &[SimulationStep], params: &impl SimulationParams) -> SimulationResult {
    #[cfg(feature = "progress_bar")]
    {
        fn handle_io_error<T>(result: std::io::Result<T>) {
            if let Err(err) = result {
                eprintln!("IO error: {err}");
            }
        }
        
        kdam::term::init(std::io::stderr().is_terminal());
        handle_io_error(kdam::term::hide_cursor());
    }

    let num_rounds = params.get_num_rounds();
    let mut simulation_rounds = Vec::with_capacity(num_rounds as usize);

    let round_iterator = (0..num_rounds).into_iter();

    #[cfg(feature = "progress_bar")]
    let round_iterator = tqdm!(round_iterator, desc="Simulation Rounds", position = 0, animation = "ascii");

    for round_number in round_iterator {
        simulation_rounds.push(
            run_simulation_round(network,
                                 simulation_steps,
                                 params,
                                 simulation_rounds.last().map(|r: &SimulationRoundResult| r.crowding_cost.as_ref()),
                                 round_number,
            )
        );
    }

    // Use the population count of the last round as the final population count.
    let last_simulation_round = simulation_rounds.last_mut().unwrap();
    let population_count = std::mem::take(&mut last_simulation_round.population_count);

    let round_agent_journeys = simulation_rounds.into_iter().map(|r| r.agent_journeys).collect();

    SimulationResult {
        population_count,
        round_agent_journeys,
    }
}
