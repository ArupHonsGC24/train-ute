mod simulation;
mod myki;

use raptor::network::{Network, StopIndex, Timestamp, TripIndex};
use raptor::raptor_query;
use raptor::utils;
use simulation::AgentCount;

use chrono::NaiveDate;
use gtfs_structures::Gtfs;
use crate::simulation::{Connection, SimulationStep};


fn main() -> Result<(), Box<dyn std::error::Error>>{

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
    let data_path = "../data/Train_Service_Passenger_Counts_Financial_Year_2022-2023.csv";
    let mut simulation_steps = myki::simulation_steps_from_csv(data_path, &network).unwrap();
    
    // Construct list of connections from trips in network.
    for route in 0..network.num_routes() {
        let num_stops = network.num_stops_in_route(route);
        for trip in 0..network.num_trips(route) {
            for stop_order in 1..num_stops {
                simulation_steps.push(SimulationStep::from_connection(Connection {
                    trip_idx: trip as TripIndex,
                    start_idx: network.get_stop_in_route(route, stop_order - 1),
                    stop_idx: network.get_stop_in_route(route, stop_order),
                    departure_time: network.get_departure_time(route, trip, stop_order - 1),
                    arrival_time: network.get_arrival_time(route, trip, stop_order),
                }));
            }
        }
    }
    
    simulation_steps.sort_unstable();
    println!("Simulation steps: {:?}", simulation_steps.len());

    for simulation_step in simulation_steps.iter() {}
    
    Ok(())
}
