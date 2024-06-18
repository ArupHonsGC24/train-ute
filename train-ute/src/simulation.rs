use rand::prelude::*;

use raptor::{Network, raptor_query};
use raptor::network::{StopIndex, Timestamp};

pub type AgentCount = u16;
pub type PopulationCount = i32;
pub type CrowdingCost = f32;

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
    pub agent_journeys: Vec<i32>,
}

pub fn gen_simulation_steps(network: &Network, seed: Option<u64>) -> Vec<AgentJourney> {
    let mut simulation_steps = Vec::new();
    let num_stops = network.num_stops() as StopIndex;
    let mut rng = match seed {
        Some(seed) => SmallRng::seed_from_u64(seed),
        None => SmallRng::from_entropy(),
    };

    // New agent journey every second.
    let sim_start_time = 4 * 60 * 60; // Start at 4am.
    let sim_end_time = 24 * 60 * 60; // Final journey begins at midnight.
    for start_time in sim_start_time..sim_end_time {
        simulation_steps.push(AgentJourney {
            start_time,
            start_stop: rng.gen_range(0..num_stops),
            end_stop: rng.gen_range(0..num_stops),
            count: rng.gen_range(1..=10),
        });
    }
    simulation_steps
}

pub fn run_simulation<T: SimulationParams>(network: &Network, simulation_steps: &[AgentJourney], params: &T) -> SimulationResult {
    // Agent counts need to be stored per trip stop, and signed so they can be temporarily negative.
    // Note: this is embarrassingly parallel, and could be done in parallel with rayon.
    let mut trip_stops_pop = vec![0 as PopulationCount; network.stop_times.len()];
    let mut trip_stops_cost = vec![0 as CrowdingCost; network.stop_times.len()];
    for journey in simulation_steps {
        let query = raptor_query(network, journey.start_stop, journey.start_time, journey.end_stop);
        for leg in query.legs {
            let route = &network.routes[leg.route_idx as usize];
            let trip = &mut trip_stops_pop[route.get_trip_range(leg.trip_idx as usize)];
            let count = journey.count as i32;
            // Add one agent to this span of trip stops.
            trip[leg.boarded_stop_order as usize] += count;
            // Remove agent at stop (for inclusive-exclusive range).
            trip[leg.arrival_stop_order as usize] -= count;
        }
    }

    // Build sums of agent counts, and calculate crowding cost.
    // Note: this ends up running through the trip_pop in order, so it's cache-friendly.
    for route_idx in 0..network.routes.len() {
        let route = &network.routes[route_idx];
        for trip in 0..route.num_trips as usize {
            let trip_range = route.get_trip_range(trip);
            let trip = &mut trip_stops_pop[trip_range.clone()];
            let costs = &mut trip_stops_cost[trip_range];

            costs[0] = params.cost_fn(trip[0]);
            for i in 0..trip.len() - 1 {
                trip[i + 1] += trip[i];
                costs[i + 1] = params.cost_fn(trip[i + 1]);
                assert!(trip[i] >= 0);
            }
        }
    }

    SimulationResult {
        agent_journeys: trip_stops_pop,
    }
}
