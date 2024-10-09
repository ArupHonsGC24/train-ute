use either::Either;
use itertools::izip;
#[cfg(feature = "progress_bar")]
use kdam::{par_tqdm, tqdm};
use rand::prelude::*;
use raptor::journey::{JourneyError, JourneyPreferences};
use raptor::network::{GlobalTripIndex, PathfindingCost, StopIndex, Timestamp};
use raptor::{Leg, Network};
use rayon::prelude::*;
use std::collections::HashMap;
#[cfg(feature = "progress_bar")]
use std::io::IsTerminal;
use std::sync::atomic::{AtomicI32, Ordering};

pub type AgentCount = u32;
pub type PopulationCount = i32;
pub type PopulationCountAtomic = AtomicI32;
pub type CrowdingCost = PathfindingCost;

#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct TripCapacity {
    pub seated: PopulationCount,
    pub standing: PopulationCount,
}

impl TripCapacity {
    pub fn total(&self) -> PopulationCount {
        self.seated + self.standing
    }
}

impl Default for TripCapacity {
    fn default() -> Self {
        // Set default capacity to 1 seated and 1 standing, so the crowding cost function is well-defined.
        Self {
            seated: 1,
            standing: 1,
        }
    }
}

#[derive(Default, Clone)]
pub struct TripCapacities {
    default: TripCapacity,
    overrides: HashMap<String, TripCapacity>,
}

impl TripCapacities {
    pub fn new(default: TripCapacity, overrides: HashMap<String, TripCapacity>) -> Self {
        Self { default, overrides }
    }

    pub fn set_default_capacity(&mut self, default: TripCapacity) {
        self.default = default;
    }

    pub fn get(&self, trip_id: &str) -> TripCapacity {
        *self.overrides.get(trip_id).unwrap_or(&self.default)
    }
}

pub type SimulationProgressCallback<'a> = dyn Fn() + Sync + Send + 'a;
pub trait SimulationParams: Sync {
    fn cost_fn(&self, trip_id: &str, count: PopulationCount) -> CrowdingCost;
    fn get_journey_preferences(&self) -> &JourneyPreferences;
    fn get_num_rounds(&self) -> u16;
    fn get_bag_size(&self) -> usize;
    fn get_progress_callback(&self) -> Option<&SimulationProgressCallback> { None }
    // Called by the simulation to report progress (0-1).
    fn run_progress_callback(&self) {
        self.get_progress_callback().map(|f| f());
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase", tag = "func", content = "params"))]
pub enum CrowdingFunc {
    Linear,
    Quadratic,
    OneStep { a0: CrowdingCost, a: CrowdingCost, b: CrowdingCost },
    TwoStep { a0: CrowdingCost, a1: CrowdingCost, a: CrowdingCost, b: CrowdingCost, c: CrowdingCost },
}

impl CrowdingFunc {
    pub fn get_name(&self) -> &'static str {
        match self {
            CrowdingFunc::Linear => "linear",
            CrowdingFunc::Quadratic => "quadratic",
            CrowdingFunc::OneStep { .. } => "one_step",
            CrowdingFunc::TwoStep { .. } => "two_step",
        }
    }

    fn linear(cap: TripCapacity, x: PopulationCount) -> CrowdingCost {
        x as CrowdingCost / cap.total() as CrowdingCost
    }

    fn quadratic(cap: TripCapacity, x: PopulationCount) -> CrowdingCost {
        let total_capacity = cap.total();
        (x * x) as CrowdingCost / (total_capacity * total_capacity) as CrowdingCost
    }

    fn one_step(cap: TripCapacity, x: PopulationCount, a0: CrowdingCost, a: CrowdingCost, b: CrowdingCost) -> CrowdingCost {
        if x == 0 {
            return 0.;
        }
        let seated = cap.seated;
        let x_on_s = x as CrowdingCost / seated as CrowdingCost;
        let s_on_x = seated as CrowdingCost / x as CrowdingCost;

        (a0 * s_on_x + (1. - s_on_x) * a0 * (1. + b * (a * (x_on_s - 1.)).exp())).max(a0)
    }

    fn two_step(cap: TripCapacity, x: PopulationCount, a0: CrowdingCost, a1: CrowdingCost, a: CrowdingCost, b: CrowdingCost, c: CrowdingCost) -> CrowdingCost {
        if x == 0 {
            return 0.;
        }

        a0 + (a1 - a0) / (1. + (a * (cap.seated - x) as CrowdingCost).exp()) + b * (c * (x - cap.total()) as CrowdingCost).exp()
    }

    pub fn crowding_cost(&self, cap: TripCapacity, count: PopulationCount) -> CrowdingCost {
        match &self {
            CrowdingFunc::Linear => Self::linear(cap, count),
            CrowdingFunc::Quadratic => Self::quadratic(cap, count),
            CrowdingFunc::OneStep { a0, a, b } => Self::one_step(cap, count, *a0, *a, *b),
            CrowdingFunc::TwoStep { a0, a1, a, b, c } => Self::two_step(cap, count, *a0, *a1, *a, *b, *c),
        }
    }

    pub fn generate_csv(&self, cap: TripCapacity) -> String {
        let mut csv = String::new();
        csv.push_str(&format!("count,{}_cost\n", self.get_name()));
        for count in 0..=cap.total() {
            let cost = self.crowding_cost(cap, count);
            csv.push_str(&format!("{count},{cost}\n"));
        }
        csv
    }
}

// This default simulation parameter implementation uses a simple exponential crowding cost function, and can report progress.
pub struct DefaultSimulationParams<'a> {
    pub crowding_function: CrowdingFunc,
    pub progress_callback: Option<Box<SimulationProgressCallback<'a>>>,
    pub journey_preferences: JourneyPreferences,
    pub num_rounds: u16,
    pub bag_size: usize,
    pub trip_capacities: TripCapacities,
}

impl SimulationParams for DefaultSimulationParams<'_> {
    fn cost_fn(&self, trip_id: &str, count: PopulationCount) -> CrowdingCost {
        debug_assert!(count >= 0, "Negative population count");
        self.crowding_function.crowding_cost(self.trip_capacities.get(trip_id), count)
    }

    fn get_journey_preferences(&self) -> &JourneyPreferences {
        &self.journey_preferences
    }

    fn get_num_rounds(&self) -> u16 {
        self.num_rounds
    }

    fn get_bag_size(&self) -> usize {
        self.bag_size
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
    pub fn len(&self) -> usize {
        self.dest_stops.len()
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
    pub start_time: Timestamp,
    pub duration: Timestamp,
    pub crowding_cost: CrowdingCost,
    pub num_transfers: u8,
    pub legs: Vec<Leg>,
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
        log::info!("Rounds: {}", self.round_agent_journeys.len());
        log::info!("Agent journeys: {}", self.round_agent_journeys.last().map(|v| v.len()).unwrap_or(0));
    }
}

pub fn gen_simulation_steps(network: &Network, number: Option<usize>, seed: Option<u64>) -> Vec<SimulationStep> {
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
    let mut simulation_steps = Vec::with_capacity(number);
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
                        round_number: u16) -> SimulationRoundResult {
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

    let num_agents = simulation_steps.iter().fold(0, |acc, step| acc + step.len());

    let step_iterator = simulation_steps.par_iter();

    #[cfg(feature = "progress_bar")]
    let step_iterator = {
        par_tqdm!(
            step_iterator,
            desc = " Simulation Steps", 
            position = round_number + 1,
            animation = kdam::Animation::FillUp
        )
    };

    // Use a bag size of 1 for the first round, because there's no crowding data yet.
    let bag_size = if round_number == 0 { 1 } else { params.get_bag_size().clamp(2, 5) };

    let mut agent_journeys = Vec::with_capacity(num_agents);
    agent_journeys.par_extend(step_iterator
        .enumerate()
        .flat_map_iter(|(sim_step_idx, sim_step)| {
            params.run_progress_callback();

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

            macro_rules! mc_raptor {
                ($bag_size:expr) => {
                    raptor::mc_raptor_query::<$bag_size>(network,
                                                         sim_step.origin_stop,
                                                         sim_step.departure_time,
                                                         &sim_step.dest_stops,
                                                         crowding_cost,
                                                         &journey_preferences)
                };
            }

            let journeys = match bag_size {
                // TODO: Implement bag size 1 with normal raptor (extend to multi-dest).
                1 => mc_raptor!(1),
                2 => mc_raptor!(2),
                3 => mc_raptor!(3),
                4 => mc_raptor!(4),
                5 => mc_raptor!(5),
                _ => unreachable!(),
            };

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
                                start_time: sim_step.departure_time,
                                duration: journey.duration,
                                crowding_cost: journey.cost,
                                num_transfers: (journey.legs.len() - 1) as u8,
                                legs: journey.legs,
                            }),
                        }
                    }))
        }));

    let mut trip_stops_cost = vec![0 as CrowdingCost; network.stop_times.len()];

    // Copy counts from Vec<PopulationCountAtomic> to Vec<PopulationCount>.
    let mut trip_stops_pop = trip_stops_pop.iter().map(|x| x.load(Ordering::Relaxed)).collect::<Vec<PopulationCount>>();

    // Build sums of agent counts, and calculate crowding cost.
    // Note: this ends up running through the trip_pop in order, so it's cache-friendly.
    for route_idx in 0..network.routes.len() {
        let route = &network.routes[route_idx];
        for trip in 0..route.num_trips as usize {
            let trip_id = &route.trip_ids[trip];
            let trip_range = route.get_trip_range(trip);
            let stop_times = &network.stop_times[trip_range.clone()];
            let trip = &mut trip_stops_pop[trip_range.clone()];
            let costs = &mut trip_stops_cost[trip_range];

            costs[0] = params.cost_fn(trip_id, trip[0]);
            for i in 0..(trip.len() - 1) {
                // Calculate prefix sums
                trip[i + 1] += trip[i];
                // Calculate crowding cost.
                let cost_per_unit_time = params.cost_fn(trip_id, trip[i + 1]);
                let connection_time = stop_times[i + 1].departure_time.checked_sub(stop_times[i].arrival_time).unwrap_or_else(|| {
                    log::warn!("Negative connection time: {} -> {}", raptor::utils::get_time_str(stop_times[i].arrival_time), raptor::utils::get_time_str(stop_times[i + 1].departure_time));
                    0
                });
                costs[i + 1] = cost_per_unit_time * connection_time as CrowdingCost;
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

pub fn run_simulation(network: &Network, simulation_steps: &[SimulationStep], params: &impl SimulationParams) -> SimulationResult {
    #[cfg(feature = "progress_bar")]
    if params.get_progress_callback().is_some() {
        fn handle_io_error<T>(result: std::io::Result<T>) {
            if let Err(err) = result {
                log::error!("IO error: {err}");
            }
        }

        kdam::term::init(std::io::stderr().is_terminal());
        handle_io_error(kdam::term::hide_cursor());
    }

    let num_rounds = params.get_num_rounds();
    let mut simulation_rounds = Vec::with_capacity(num_rounds as usize);

    let round_iterator = (0..num_rounds).into_iter();
    let mut run_round = |round_number| {
        simulation_rounds.push(
            run_simulation_round(network,
                                 simulation_steps,
                                 params,
                                 simulation_rounds.last().map(|r: &SimulationRoundResult| r.crowding_cost.as_ref()),
                                 round_number,
            )
        );
    };

    #[cfg(feature = "progress_bar")]
    if params.get_progress_callback().is_some() {
        for round_number in tqdm!(round_iterator, desc = "Simulation Rounds", position = 0) {
            run_round(round_number);
        }
    } else {
        for round_number in round_iterator {
            run_round(round_number);
        }
    }

    #[cfg(not(feature = "progress_bar"))]
    for round_number in round_iterator {
        run_round(round_number);
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
