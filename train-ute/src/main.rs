use std::time::Instant;

use chrono::NaiveDate;
use gtfs_structures::GtfsReader;

use raptor::network::{Network, TripIndex};
use raptor::raptor_query;
use raptor::utils;

use crate::simulation::{Connection, run_simulation, SimulationParams, SimulationStep};

mod simulation;
mod data_import;
mod data_export;
mod simulationv2;
mod colour;

// Simulation notes:
// When we get the O-D data, we can run journey planning for each OD and apply the passenger counts to the relevant trips.
// once this is run once, we update the journey planning weights based on the crowding and run again.
// This is like the 'El Farol Bar' problem.
// Matsim-like replanning for a proportion of the population might also be viable.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gtfs_start = Instant::now();
    let gtfs = GtfsReader::default().read_shapes(true).read("../gtfs/2/google_transit.zip")?;
    let gtfs_duration = Instant::now() - gtfs_start;
    println!("GTFS import: {gtfs_duration:?}");

    let journey_date = NaiveDate::from_ymd_opt(2024, 5, 10).unwrap();
    let default_transfer_time = 3 * 60;
    let network_start = Instant::now();
    let mut network = Network::new(&gtfs, journey_date, default_transfer_time);
    let network_duration = Instant::now() - network_start;
    println!("Network parse: {network_duration:?}");

    let start = network.get_stop_idx("15351");
    let end = network.get_stop_idx("19891");
    let start_time = utils::parse_time("8:30:00")?;

    let journey = raptor_query(&network, start, start_time, end);
    println!("{journey}");

    // Set up simulation
    let params = SimulationParams {
        // From VicSig: X'Trapolis 3-car has 264 seated, 133 standing. A 6-car has 794 in total.
        // Crush capacity is 1394, but that's a bit mean.
        // https://vicsig.net/suburban/train/X'Trapolis
        max_train_capacity: 794,
    };

    // Add steps to spawn and delete agents based on data.
    //let data_path = "../data/Train_Service_Passenger_Counts_Financial_Year_2022-2023.parquet";
    //// HACK: Change network date to the year before so we have myki data for it.
    //network.date -= chrono::Duration::days(365);

    //let data_parse_start = Instant::now();
    //let mut simulation_steps = data_import::gen_simulation_steps(data_path, &network)?;
    //let data_parse_duration = Instant::now() - data_parse_start;
    //println!("Data parse duration: {data_parse_duration:?}");

    //// Construct list of connections from trips in network.
    //for route in 0..network.num_routes() {
    //    let num_stops = network.num_stops_in_route(route);
    //    for trip in 0..network.num_trips(route) {
    //        for stop_order in 1..num_stops {
    //            simulation_steps.push(SimulationStep::from_connection(Connection {
    //                trip_idx: trip as TripIndex,
    //                start_idx: network.get_stop_in_route(route, stop_order - 1),
    //                stop_idx: network.get_stop_in_route(route, stop_order),
    //                departure_time: network.get_departure_time(route, trip, stop_order - 1),
    //                arrival_time: network.get_arrival_time(route, trip, stop_order),
    //            }));
    //        }
    //    }
    //}

    //simulation_steps.sort_unstable();
    //println!("Num simulation steps: {:?}", simulation_steps.len());
    
    let simulation_steps = simulationv2::gen_simulation_steps(&network, Some(0));

    //// Run simulation

    let simulation_start = Instant::now();
    //let simulation_result = run_simulation(&network, &simulation_steps, &params);
    let simulation_result = simulationv2::run_simulation_v2(&network, &simulation_steps, params);
    let simulation_duration = Instant::now() - simulation_start;
    println!("Simulation duration: {simulation_duration:?}");

    //println!("Num agent transfers: {}", simulation_result.agent_transfers.len());

    // Print a sample.
    //for agent_transfer in simulation_result.agent_transfers.iter().skip(10000).take(10) {
    //    println!("Agent transfer: {}", agent_transfer.count);
    //}

    println!("Exporting results.");
    let export_start = Instant::now();
    
    data_export::export_shape_file("../train-vis/src/data/shapes.bin.zip", &network)?;
    data_export::export_network_trips("../train-vis/src/data/trips.bin.zip", &network, &simulation_result)?;
    //data_export::export_agent_transfers("../train-vis/src/data/transfers.bin.zip", &network, &simulation_result.agent_transfers)?;
    
    let export_duration = Instant::now() - export_start;
    println!("Export duration: {export_duration:?}");
    
    println!();
    println!("Total time: {:?}", Instant::now() - gtfs_start);

    Ok(())
}
