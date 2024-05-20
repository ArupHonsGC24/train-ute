use std::cmp::Ordering;

use raptor::{Network, raptor_query};
use raptor::network::{StopIndex, Timestamp, TripIndex};

pub type AgentCount = u16;

#[derive(Debug)]
pub struct Connection {
    pub trip_idx: TripIndex,
    pub start_idx: StopIndex,
    pub stop_idx: StopIndex,
    pub departure_time: Timestamp,
    pub arrival_time: Timestamp,
}

#[derive(Debug)]
pub enum SimulationOp {
    SpawnAgents {
        stop_idx: StopIndex,
        count: AgentCount,
    },
    DeleteAgents {
        stop_idx: StopIndex,
        count: AgentCount,
    },
    RunConnection(Connection),
}

#[derive(Debug)]
pub struct SimulationStep {
    pub time: Timestamp,
    pub op: SimulationOp,
}

impl Eq for SimulationStep {}

impl PartialEq for SimulationStep {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time
    }
}

impl PartialOrd for SimulationStep {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SimulationStep {
    fn cmp(&self, other: &Self) -> Ordering {
        self.time.cmp(&other.time)
    }
}

impl SimulationStep {
    pub fn from_connection(connection: Connection) -> Self {
        Self {
            time: connection.departure_time,
            op: SimulationOp::RunConnection(connection),
        }
    }
}

pub struct SimulationParams {
    pub max_train_capacity: AgentCount,
}

pub struct AgentTransfer {
    pub timestamp: Timestamp,
    pub arrival_time: Timestamp,
    pub start_idx: StopIndex,
    pub end_idx: StopIndex,
    pub count: AgentCount,
}

pub struct SimulationResult {
    pub agent_transfers: Vec<AgentTransfer>,
}

pub fn run_simulation(network: &Network, sorted_steps: &[SimulationStep], params: &SimulationParams) -> SimulationResult {
    // Number of people at each stop of the network.
    let mut stop_pop = vec![0 as AgentCount; network.num_stops()];
    // Number of people at each trip of the network.
    // let mut trip_pop = vec![0 as AgentCount; gtfs.trips.len()];

    let dest = network.get_stop_idx_from_name("Flinders Street").unwrap();

    let mut agent_transfers = Vec::new();
    for simulation_step in sorted_steps.iter() {
        let timestamp = simulation_step.time;
        match &simulation_step.op {
            SimulationOp::SpawnAgents { stop_idx, count } => {
                // Dummy query for benchmarking
                let _ = raptor_query(network, *stop_idx, simulation_step.time, dest);
                stop_pop[*stop_idx as usize] += count;
            }
            SimulationOp::DeleteAgents { stop_idx, count } => {
                let stop_idx = *stop_idx as usize;
                stop_pop[stop_idx] = stop_pop[stop_idx].saturating_sub(*count);
                //stop_pop[stop_idx] = match stop_pop[stop_idx].checked_sub(*count) {
                //    Some(val) => val,
                //    None => {
                //        eprintln!("Negative agent count at stop {} at time {time_str}", network.get_stop(stop_idx).name);
                //        0 as AgentCount
                //    },
                //};
            }
            SimulationOp::RunConnection(connection) => {
                // Simplest model: all agents on are moved from start to stop.
                let start_idx = connection.start_idx as usize;
                let num_agents_moved = stop_pop[start_idx].min(params.max_train_capacity);
                
                // Only record active transfers.
                if num_agents_moved > 0 {
                    stop_pop[connection.stop_idx as usize] += num_agents_moved;
                    stop_pop[start_idx] -= num_agents_moved;

                    agent_transfers.push(AgentTransfer {
                        /* Simulation timestamp is departure time. */
                        timestamp,
                        arrival_time: connection.arrival_time,
                        start_idx: connection.start_idx,
                        end_idx: connection.stop_idx,
                        count: num_agents_moved,
                    });
                }
            }
        }
    }

    SimulationResult { agent_transfers }
}
