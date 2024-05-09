use std::cmp::Ordering;
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
