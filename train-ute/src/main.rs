use raptor::raptor_query;
use raptor::utils;

use chrono::NaiveDate;
use gtfs_structures::Gtfs;
use raptor::network::{Network, StopIndex, Timestamp, TripIndex};

struct Connection {
    trip_idx: TripIndex,
    start_idx: StopIndex,
    stop_idx: StopIndex,
    departure_time: Timestamp,
    arrival_time: Timestamp,
}

type AgentCount = u16;

enum SimulationOp {
    SpawnAgents {
        stop_idx: StopIndex,
        count: AgentCount,
    },
    DeleteAgents {
        stop_idx: StopIndex,
        count: AgentCount,
    },
    RunConnection(Connection)
}

struct SimulationStep {
    time: Timestamp,
    op: SimulationOp,
}
// Impl ordering for SimulationStep.

fn main() {
    let gtfs = Gtfs::new("../gtfs/2/google_transit.zip").unwrap();

    let journey_date = NaiveDate::from_ymd_opt(2024, 4, 29).unwrap();
    let default_transfer_time = 3 * 60;
    let network = Network::new(&gtfs, journey_date, default_transfer_time);

    let start = network.get_stop_idx("15351");
    let end = network.get_stop_idx("19891");
    let start_time = utils::parse_time("8:30:00").unwrap();

    let journey = raptor_query(&network, start, start_time, end);
    println!("{journey}");

    // Number of people at each stop of the network.
    let mut stop_pop = vec![0 as AgentCount; network.num_stops()];
    // Number of people at each trip of the network.
    let mut trip_pop = vec![0 as AgentCount; gtfs.trips.len()];

    // Add steps to spawn and delete agents based on data.

    // Construct list of connections from trips in network.
    let mut connections = Vec::new();
    for route in 0..network.num_routes() {
        let num_stops = network.num_stops_in_route(route);
        for trip in 0..network.num_trips(route) {
            for stop_order in 1..num_stops {
               connections.push(Connection {
                   trip_idx: trip as TripIndex,
                   start_idx: network.get_stop_in_route(route, stop_order - 1),
                   stop_idx: network.get_stop_in_route(route, stop_order),
                   departure_time: network.get_departure_time(route, trip, stop_order - 1),
                   arrival_time: network.get_arrival_time(route, trip, stop_order),
               });
            }
        }
    }
    connections.sort_unstable_by(|a, b| a.departure_time.cmp(&b.departure_time));
    println!("Connections: {:?}", connections.len());

    for connection in connections.iter() {

    }

}
