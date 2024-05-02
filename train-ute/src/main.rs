use raptor::Raptor;
use raptor::utils;
use raptor::raptor::Timestamp;

use chrono::NaiveDate;
use gtfs_structures::Gtfs;


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
    let default_transfer_time = 3 * 60;
    let gtfs = Gtfs::new("../gtfs/2/google_transit.zip").unwrap();
    let journey_date = NaiveDate::from_ymd_opt(2024, 4, 29).unwrap();
    let raptor = Raptor::new(&gtfs, journey_date, default_transfer_time);

    let start = raptor.get_stop_idx("15351");
    let end = raptor.get_stop_idx("19891");
    let start_time = utils::parse_time("8:30:00").unwrap();

    let journey = raptor.query(start, start_time, end);
    raptor.print_journey(&journey);

    // Number of people at each stop of the network.
    let mut stop_pop = vec![0u16; raptor.num_stops()];
    // Number of people at each trip of the network.
    let mut trip_pop = vec![0u16; gtfs.trips.len()];
    
    // Construct list of connections from trips in Raptor.
    for route in 0..raptor.num_routes() {
        let num_stops = raptor.num_stops_in_route(route);
        for trip in 0..raptor.num_trips(route) {
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
