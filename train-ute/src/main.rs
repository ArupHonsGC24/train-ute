use raptor::raptor_query;
use raptor::utils;

use chrono::NaiveDate;
use gtfs_structures::Gtfs;
use raptor::network::{Network, Timestamp};

type StopIndex = u8;
type TripIndex = u16;

struct Connection {
    trip_idx: TripIndex,
    start_idx: StopIndex,
    stop_idx: StopIndex,
    departure_time: Timestamp,
    arrival_time: Timestamp,
}

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
    let mut stop_pop = vec![0u16; network.num_stops()];
    // Number of people at each trip of the network.
    let mut trip_pop = vec![0u16; gtfs.trips.len()];
    
    // Construct list of connections from trips in Raptor.
    for route in 0..network.num_routes() {
        let num_stops = network.num_stops_in_route(route);
        for trip in 0..network.num_trips(route) {
            for stop in 1..num_stops {
                
            }
        }
    }

    let mut trips = gtfs.trips.values().collect::<Vec<_>>();
    // Sort by departure time.
    trips.sort_unstable_by(|a, b| a.stop_times[0].departure_time.cmp(&b.stop_times[0].departure_time));

    // Perhaps run simulation like CSA? Once for every connection on the network throughout the day.
    // Each step transfers people between trips and stops.
    let mut i = 0;
    for trip in trips {
        if gtfs.calendar[&trip.service_id].valid_weekday(journey_date) {
            println!("ID: {}, departure time: {}", trip.id, utils::get_time_str(trip.stop_times[0].departure_time.unwrap()));
            i += 1;
        }
        if i > 10 {
            break;
        }
    }
}
