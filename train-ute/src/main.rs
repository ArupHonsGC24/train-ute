use raptor::Raptor;
use raptor::utils;

use chrono::NaiveDate;
use gtfs_structures::Gtfs;

fn main() {
    let default_transfer_time = 30;
    let gtfs = Gtfs::new("../gtfs/2/google_transit.zip").unwrap();
    let journey_date = NaiveDate::from_ymd_opt(2024, 4, 29).unwrap();
    let raptor = Raptor::new(&gtfs, journey_date, default_transfer_time);

    let start = raptor.get_stop_idx("15351");
    let end = raptor.get_stop_idx("19891");

    let journey = raptor.query(start, utils::parse_time("8:30:00").unwrap(), end);
    raptor.print_journey(&journey);
}
