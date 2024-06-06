use rand::prelude::*;

use raptor::{Network, raptor_query};
use raptor::network::{StopIndex, Timestamp};

use crate::simulation::{AgentCount, SimulationParams};

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

pub fn run_simulation_v2(network: &Network, simulation_steps: &[AgentJourney], _params: SimulationParams) -> SimulationResult {
    // Agent counts need to be stored per trip stop, and signed so they can be temporarily negative.
    // Note: this is embarrassingly parallel, and could be done in parallel with rayon.
    let mut trip_stops_pop = vec![0i32; network.stop_times.len()];
    for journey in simulation_steps {
        let query = raptor_query(network, journey.start_stop, journey.start_time, journey.end_stop);
        for leg in query.legs {
            let route = &network.routes[leg.route_idx as usize];
            let trip = &mut trip_stops_pop[route.get_trip_range(leg.trip_idx as usize)];
            let count = journey.count as i32;
            // Add one agent to this span of trip stops.
            trip[leg.boarded_stop_order as usize] += count;

            // Cap range this agent is added to by subtracting from one past its last stop (if it exists)
            let agent_range_cap = leg.arrival_stop_order as usize + 1;
            if agent_range_cap < trip.len() {
                trip[agent_range_cap] -= count;
            }
        }
    }

    // Build sums of agent counts.
    // Note: there may be a way to do this with one linear sweep, instead of jumping all over the place.
    for route_idx in 0..network.routes.len() {
        let route = &network.routes[route_idx];
        for trip in 0..route.num_trips as usize {
            let trip_range = route.get_trip_range(trip);
            let trip = &mut trip_stops_pop[trip_range];
            for i in 0..trip.len() - 1 {
                trip[i + 1] += trip[i];
                assert!(trip[i] >= 0);
            }
        }
    }

    SimulationResult {
        agent_journeys: trip_stops_pop,
    }
}