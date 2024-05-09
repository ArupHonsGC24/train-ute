use std::fs::File;
use std::io::{BufRead, BufReader};
use chrono::NaiveDate;
use serde::Deserialize;
use tqdm::Iter;
use raptor::Network;
use crate::simulation::{SimulationOp, SimulationStep};

#[derive(Deserialize)]
struct PassengerCountRecord {
    #[serde(rename = "")]
    line_num: u32,
    #[serde(rename = "Business_Date")]
    business_date: NaiveDate,
    #[serde(rename = "Day_of_Week")]
    day_of_week: String,
    #[serde(rename = "Day_Type")]
    day_type: String,
    #[serde(rename = "Mode")]
    mode: String,
    #[serde(rename = "Train_Number")]
    train_num: u32,
    #[serde(rename = "Line_Name")]
    line: String,
    #[serde(rename = "Group")]
    group: String,
    #[serde(rename = "Direction")]
    direction: String,
    #[serde(rename = "Origin_Station")]
    origin_station: String,
    #[serde(rename = "Destination_Station")]
    destination_station: String,
    #[serde(rename = "Station_Name")]
    station_name: String,
    #[serde(rename = "Station_Latitude")]
    station_lat: f64,
    #[serde(rename = "Station_Longitude")]
    station_long: f64,
    #[serde(rename = "Station_Chainage")]
    station_chainage: u32,
    #[serde(rename = "Stop_Sequence_Number")]
    stop_seq_num: u32,
    #[serde(rename = "Arrival_Time_Scheduled")]
    arrival_time_scheduled: String,
    #[serde(rename = "Departure_Time_Scheduled")]
    departure_time_scheduled: String,
    #[serde(rename = "Passenger_Boardings")]
    passenger_boardings: u16,
    #[serde(rename = "Passenger_Alightings")]
    passenger_alightings: u16,
    #[serde(rename = "Passenger_Arrival_Load")]
    passenger_arrival_load: u16,
    #[serde(rename = "Passenger_Departure_Load")]
    passenger_departure_load: u16,
}
fn count_lines(path: &str) -> Result<usize, std::io::Error> {
    let mut lines = BufReader::new(File::open(&path)?).lines();
    // Set breakpoint on the next line, pull the drive, then continue
    let count = lines.try_fold(0, |acc, line| line.map(|_| acc + 1))?;
    Ok(count)
}
pub fn simulation_steps_from_csv(path: &str, network: &Network) -> Result<Vec<SimulationStep>, csv::Error> {
    let num_lines = count_lines(path).unwrap();
    println!("Number of lines: {}", num_lines);
    let mut reader = csv::Reader::from_path(path)?;

    let mut simulation_steps = Vec::new();
    for record in reader.deserialize::<PassengerCountRecord>().flatten().take(num_lines).tqdm() {
        if record.passenger_boardings == 0 && record.passenger_alightings == 0 {
            continue;
        }
        if record.mode != "Metro" {
            continue;
        }
        let stop_idx = network.get_stop_idx_from_name(&record.station_name);
        let stop_idx = match stop_idx {
            Some(idx) => idx,
            None => {
                eprintln!("Station not found: {}", record.station_name);
                continue;
            }
        };
        let time = raptor::utils::parse_time(&record.departure_time_scheduled).unwrap();
        simulation_steps.push(SimulationStep {
            time,
            op: SimulationOp::SpawnAgents {
                stop_idx,
                count: record.passenger_boardings,
            }
        });
        simulation_steps.push(SimulationStep {
            time,
            op: SimulationOp::DeleteAgents {
                stop_idx,
                count: record.passenger_alightings,
            }
        });
    }

    Ok(simulation_steps)
}