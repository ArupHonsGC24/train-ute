use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, Float32Array, StringArray, Time32MillisecondArray, TimestampMillisecondArray, UInt32Array};
use arrow::datatypes::{Field, Schema};
use itertools::{izip, Itertools};
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use rgb::RGB8;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::simulation::{SimulationResult, TripCapacities};
use crate::utils::{mix_rgb, quadratic_ease_in_out, quadratic_inv_ease_in_out};
use raptor::journey::JourneyError;
use raptor::network::{CoordType, NetworkPoint, Timestamp};
use raptor::utils::get_time_str;
use raptor::Network;

#[derive(thiserror::Error, Debug)]
pub enum DataExportError {
    #[error("No data to export.")]
    NoData,
    #[error("IO error: {0}.")]
    IoError(#[from] std::io::Error),
    #[error("Arrow error: {0}.")]
    ArrowError(#[from] arrow::error::ArrowError),
    #[error("Parquet error: {0}.")]
    ParquetError(#[from] parquet::errors::ParquetError),
    #[error("CSV error: {0}.")]
    CsvError(#[from] csv::Error),
}

// Writes a set of binary data to a writer in a simple format:
// - A 32-bit byte offset and length for each data chunk.
// - The binary data chunks, each aligned to 8 bytes.
pub fn write_bin(data_list: &[&[u8]], writer: &mut impl Write) -> std::io::Result<()> {
    // Simple power-of-two alignment.
    fn round_up_to_eight(num: usize) -> usize { (num + 7) & !7 }

    // A 32-bit byte offset and length for each data chunk, followed by the data chunks.
    // We want the data to be aligned to 8 bytes.
    let header_size = data_list.len() * 2 * size_of::<u32>(); // 2 32-bit values per data chunk.
    let mut index = header_size as u32; // Start past header.
    let mut written_bytes = 0;
    for &data in data_list {
        written_bytes += writer.write(&index.to_le_bytes())?;
        written_bytes += writer.write(&(data.len() as u32).to_le_bytes())?;
        index += round_up_to_eight(data.len()) as u32;
    }

    // Sanity check.
    assert_eq!(written_bytes, header_size);

    // Write data, maintaining 8-byte alignment.
    for &data in data_list {
        writer.write_all(data)?;
        let padding = round_up_to_eight(data.len()) - data.len();
        for _ in 0..padding {
            writer.write_all(&0u8.to_le_bytes())?;
        }
    }

    Ok(())
}

// Writes a set of binary data to a zip file.
pub fn open_zip(path: &Path) -> std::io::Result<ZipWriter<File>> {
    // Open zip file.
    let mut zip = ZipWriter::new(File::create(path)?);
    zip.start_file("data.bin", SimpleFileOptions::default())?;
    Ok(zip)
}

pub fn export_shape_file(network: &Network, writer: &mut impl Write) -> Result<(), DataExportError> {
    let mut shape_points = Vec::new();
    let mut shape_start_indices = Vec::new();
    let mut shape_colours = Vec::new();

    for route in network.routes.iter() {
        let colour = route.colour;
        let height = route.shape_height;

        // Indices are based on points, not coordinates.
        shape_start_indices.push(shape_points.len() as u32 / 3);

        // Construct line string from shape.
        for point in route.shape.iter() {
            shape_points.push(point.longitude);
            shape_points.push(point.latitude);
            shape_points.push(height);

            shape_colours.push(colour.r);
            shape_colours.push(colour.g);
            shape_colours.push(colour.b);
        }
    }

    write_bin(&[bytemuck::must_cast_slice(&shape_points), bytemuck::must_cast_slice(&shape_start_indices), &shape_colours], writer)?;

    Ok(())
}

pub fn export_network_trips(network: &Network, simulation_result: &SimulationResult, writer: &mut impl Write) -> Result<(), DataExportError> {
    const NUM_COORDS_PER_POINT: u32 = 3;

    // I haven't bothered to calculate capacities, but it's amortised constant to push anyway so there's not really any point.
    let mut start_indices = Vec::new();
    let mut trip_points = Vec::new();
    let mut trip_times = Vec::new();
    let mut trip_colours = Vec::new();

    for route_idx in 0..network.num_routes() {
        let num_stops = network.num_stops_in_route(route_idx);
        let route = &network.routes[route_idx];
        let route_shape = &route.shape;
        let height = route.shape_height;

        // Colour blind friendly colours from https://davidmathlogic.com/colorblind/#%23005AB5-%23DC3220
        const LOW_COLOUR: RGB8 = RGB8 { r: 0, g: 90, b: 181 };
        const HIGH_COLOUR: RGB8 = RGB8 { r: 220, g: 50, b: 32 };

        for trip_idx in 0..network.num_trips(route_idx) {
            start_indices.push(trip_points.len() as u32 / NUM_COORDS_PER_POINT);

            let agent_counts = &simulation_result.population_count[route.get_trip_range(trip_idx)];

            let mut shape_idx = 0;
            for dep_stop_order in 0..num_stops - 1 {
                let arr_stop_order = dep_stop_order + 1;

                let departure_time = network.get_departure_time(route_idx, trip_idx, dep_stop_order) as f32;

                let arr_stop_idx = network.get_stop_in_route(route_idx, arr_stop_order) as usize;
                let arr_point = network.stop_points[arr_stop_idx];
                let arrival_time = network.get_arrival_time(route_idx, trip_idx, arr_stop_order) as f32;

                // Calculate alpha based on agent count.
                let dep_count = agent_counts[dep_stop_order];

                // Ignore trips with no agents.
                assert!(dep_count >= 0);
                if dep_count == 0 {
                    continue;
                }
                let dep_count = dep_count as f32;
                let arr_count = agent_counts[arr_stop_order] as f32;
                let agent_count_diff = arr_count - dep_count;

                const MAX_AGENT_COUNT: f32 = 50.;

                let mut push_point = |point: NetworkPoint, next_point: NetworkPoint| {
                    // Location is offset to the left to separate inbound and outbound.
                    const OFFSET: CoordType = 20.;
                    let offset_point = point.left_offset(next_point, OFFSET);
                    trip_points.push(offset_point.longitude);
                    trip_points.push(offset_point.latitude);
                    trip_points.push(height);
                };

                // Go through shape points and add to point list.
                let start_shape_idx = shape_idx;
                let mut current_point = route_shape[shape_idx];
                let mut distance_along_shape_section = 0. as CoordType;
                while !current_point.very_close(arr_point) {
                    if route_shape.len() <= shape_idx + 1 {
                        log::warn!("Warning: Shape index out of bounds for route {}, stop {}({arr_stop_order}).", network.routes[route_idx].line, network.stops[arr_stop_idx].name);
                        break;
                    }

                    shape_idx += 1;
                    let next_point = route_shape[shape_idx];
                    distance_along_shape_section += current_point.distance(next_point);

                    push_point(current_point, next_point);

                    current_point = next_point;
                }

                // Push the arrival point.
                shape_idx += 1;
                if shape_idx < route_shape.len() {
                    push_point(current_point, route_shape[shape_idx]);
                } else {
                    push_point(arr_point, arr_point);
                }

                // Calculate time based on distance proportion.
                let section_duration = arrival_time - departure_time;
                let mut distance = 0.;
                for shape_idx in start_shape_idx..shape_idx {
                    assert!(distance >= 0.);

                    // Calculate proportion along this shape we are, for interpolating properties.
                    // Apply an easing function to the proportion, so trains accelerate and decelerate.
                    // We use the inverse of the easing function for easing time.
                    let proportion = if distance_along_shape_section <= 0. {
                        if shape_idx < route_shape.len() { 0. } else { 1. }
                    } else {
                        (distance / distance_along_shape_section) as f32
                    };

                    let proportion_inv = quadratic_inv_ease_in_out(proportion);
                    let proportion = quadratic_ease_in_out(proportion);
                    let time = departure_time + section_duration * proportion_inv;
                    trip_times.push(time);

                    // Colour (RGBA). Calculate alpha based on agent count.
                    let value = (dep_count + agent_count_diff * proportion) / MAX_AGENT_COUNT;
                    let shape_colour = mix_rgb(LOW_COLOUR, HIGH_COLOUR, value);

                    trip_colours.push(shape_colour.r);
                    trip_colours.push(shape_colour.g);
                    trip_colours.push(shape_colour.b);
                    trip_colours.push(255);

                    let segment_distance = if shape_idx + 1 < route_shape.len() {
                        route_shape[shape_idx].distance(route_shape[shape_idx + 1])
                    } else {
                        0.
                    };

                    distance += segment_distance;
                }

                // This is required so we count the last point as the start of the next section.
                shape_idx -= 1;

                assert_eq!(trip_points.len(), trip_times.len() * NUM_COORDS_PER_POINT as usize);
            }
        }
    }

    write_bin(&[bytemuck::must_cast_slice(&trip_points), bytemuck::must_cast_slice(&start_indices), bytemuck::must_cast_slice(&trip_times), &trip_colours], writer)?;

    Ok(())
}

// Exports the agent counts to a parquet (and csv) file.
pub fn export_agent_counts(path: &Path, network: &Network, simulation_result: &SimulationResult, trip_capacities: &TripCapacities) -> Result<(), DataExportError> {
    let path = path.with_extension("parquet");

    // This is the utc timestamp for the midnight of the day the network represents.
    let date_timestamp = network.date.and_time(chrono::NaiveTime::MIN).and_utc().timestamp();

    let mut trip_ids = Vec::new();
    let mut trip_seated = Vec::new();
    let mut trip_standing = Vec::new();
    let mut timestamps = Vec::new(); // Unix timestamps in milliseconds.
    let mut departures = Vec::new();
    let mut departure_ids = Vec::new();
    let mut arrivals = Vec::new();
    let mut arrival_ids = Vec::new();
    let mut agent_counts = Vec::new();

    for route in network.routes.iter() {
        for trip in 0..route.num_trips as usize {
            let trip_id = route.trip_ids[trip].as_ref();
            let trip_capacity = trip_capacities.get(trip_id);
            let trip_range = route.get_trip_range(trip);

            let stop_times_ms = network.stop_times[trip_range.clone()].iter().map(|stop_time| {
                (date_timestamp + stop_time.departure_time as i64) * 1000 // Convert to milliseconds, as seconds is not as widely supported.
            });
            let stops = route.get_stops(&network.route_stops).iter().tuple_windows();
            let trip_agent_counts = &simulation_result.population_count[trip_range.clone()];

            for ((&dep_stop_idx, &arr_stop_idx), time_ms, &agent_count) in izip!(stops, stop_times_ms, trip_agent_counts) {
                trip_ids.push(trip_id);
                trip_seated.push(trip_capacity.seated as u32);
                trip_standing.push(trip_capacity.standing as u32);
                timestamps.push(time_ms);
                departures.push(network.stops[dep_stop_idx as usize].name.as_ref());
                departure_ids.push(network.stops[dep_stop_idx as usize].id.as_ref());
                arrivals.push(network.stops[arr_stop_idx as usize].name.as_ref());
                arrival_ids.push(network.stops[arr_stop_idx as usize].id.as_ref());
                assert!(agent_count >= 0, "Negative agent count: {}", agent_count);
                agent_counts.push(agent_count as u32);
            }
        }
    }

    // Set up arrow arrays.
    let trip_id_arr = Arc::new(StringArray::from(trip_ids.clone()));
    let trip_id_field = Field::new("trip_name", trip_id_arr.data_type().clone(), false);

    let trip_seated_arr = Arc::new(UInt32Array::from(trip_seated.clone()));
    let trip_seated_field = Field::new("seated_capacity", trip_seated_arr.data_type().clone(), false);

    let trip_standing_arr = Arc::new(UInt32Array::from(trip_standing.clone()));
    let trip_standing_field = Field::new("standing_capacity", trip_standing_arr.data_type().clone(), false);

    let timestamps_arr = Arc::new(TimestampMillisecondArray::from(timestamps.clone()));
    let timestamp_field = Field::new("timestamp", timestamps_arr.data_type().clone(), false);

    let departures_arr = Arc::new(StringArray::from(departures.clone()));
    let departures_field = Field::new("departure", departures_arr.data_type().clone(), false);

    let departure_ids_arr = Arc::new(StringArray::from(departure_ids.clone()));
    let departure_ids_field = Field::new("departure_id", departure_ids_arr.data_type().clone(), false);

    let arrivals_arr = Arc::new(StringArray::from(arrivals.clone()));
    let arrivals_field = Field::new("arrival", arrivals_arr.data_type().clone(), false);

    let arrival_ids_arr = Arc::new(StringArray::from(arrival_ids.clone()));
    let arrival_ids_field = Field::new("arrival_id", arrival_ids_arr.data_type().clone(), false);

    let agent_counts_arr = Arc::new(UInt32Array::from(agent_counts.clone()));
    let agent_counts_field = Field::new("count", agent_counts_arr.data_type().clone(), false);

    let schema = Arc::new(Schema::new(vec![
        trip_id_field,
        trip_seated_field,
        trip_standing_field,
        timestamp_field,
        departures_field,
        departure_ids_field,
        arrivals_field,
        arrival_ids_field,
        agent_counts_field
    ]));

    // TODO: A record batch per trip? Sort trips by earliest departure time?
    let record_batch = arrow::record_batch::RecordBatch::try_new(schema, vec![
        trip_id_arr,
        trip_seated_arr,
        trip_standing_arr,
        timestamps_arr,
        departures_arr,
        departure_ids_arr,
        arrivals_arr,
        arrival_ids_arr,
        agent_counts_arr
    ])?;

    // Write to parquet.
    {
        let props = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .build();
        let mut writer = ArrowWriter::try_new(File::create(&path)?, record_batch.schema(), Some(props))?;

        writer.write(&record_batch)?;

        writer.close()?;
    }

    // Write to csv (for debugging).
    {
        let mut csv_writer = csv::Writer::from_path(path.with_extension("csv"))?;
        csv_writer.write_record(&["trip_id", "timestamp", "departure", "departure_id", "arrival", "arrival_id", "count"])?;
        let date_str = network.date.to_string();
        for (trip_name, timestamp, departure, departure_id, arrival, arrival_id, count) in izip!(trip_ids, timestamps, departures, departure_ids, arrivals, arrival_ids, agent_counts) {
            let timestamp = format!("{date_str} {}", &get_time_str((timestamp / 1000 - date_timestamp) as Timestamp));
            csv_writer.write_record(&[trip_name, &timestamp, departure, departure_id, arrival, arrival_id, &count.to_string()])?;
        }
    }

    Ok(())
}

pub fn export_stops_csv(path: &Path, network: &Network) -> Result<(), DataExportError> {
    // Write stops CSV.
    let csv_path = path.with_extension("csv");
    let mut csv_writer = csv::Writer::from_path(csv_path)?;
    // Stop ID, Name, Latitude, Longitude.
    csv_writer.write_record(&["id", "name", "latitude", "longitude"])?;
    for (location, stop) in network.stops.iter().enumerate().map(|(i, stop)| (network.stop_points[i], stop)) {
        csv_writer.write_record(&[&stop.id.to_string(), &stop.name.to_string(), &location.latitude.to_string(), &location.longitude.to_string()])?;
    }

    Ok(())
}

pub fn export_agent_journeys(writer: impl Write + Send, network: &Network, simulation_result: &SimulationResult, legs: bool) -> Result<(), DataExportError> {
    let num_records = simulation_result.round_agent_journeys.iter().fold(0, |acc, journeys| acc + journeys.len());

    let num_agents = simulation_result.round_agent_journeys.first().ok_or(DataExportError::NoData)?.len();
    for agent_journeys in simulation_result.round_agent_journeys.iter() {
        assert_eq!(agent_journeys.len(), num_agents, "Agent journey count does not match agent count.");
    }

    // Note: when legs = true, these are underestimated capacities.
    let mut agent_ids = Vec::with_capacity(num_records);
    let mut status = Vec::with_capacity(num_records);
    let mut round_number = Vec::with_capacity(num_records);

    let mut origins = Vec::with_capacity(num_records);
    let mut origin_trip_ids = Vec::with_capacity(num_records);
    let mut destinations = Vec::with_capacity(num_records);
    let mut destination_trip_ids = Vec::with_capacity(num_records);

    let mut journey_times_ms = Vec::with_capacity(num_records);
    let mut journey_start_times_ms = Vec::with_capacity(num_records);
    let mut crowding_costs = Vec::with_capacity(num_records);
    let mut num_transfers = Vec::with_capacity(num_records);

    // Convert timestamps to milliseconds because the Time32Second type is not widely supported.
    fn sec_to_milli(sec: Timestamp) -> i32 {
        sec as i32 * 1000
    }

    for i in 0..num_agents {
        for round in 0..simulation_result.round_agent_journeys.len() {
            let journey = &simulation_result.round_agent_journeys[round][i];
            match &journey.result {
                Ok(journey) => {
                    if legs {
                        for leg in &journey.legs {
                            agent_ids.push(i as u32);
                            status.push("Ok");
                            round_number.push(Some(round as u32));

                            origins.push(Some(network.stops[leg.boarded_stop as usize].name.as_ref()));
                            origin_trip_ids.push(Some(network.get_trip_id(leg.trip)));
                            destinations.push(Some(network.stops[leg.arrival_stop as usize].name.as_ref()));

                            journey_times_ms.push(Some(sec_to_milli(leg.arrival_time - leg.boarded_time)));
                            journey_start_times_ms.push(Some(sec_to_milli(leg.boarded_time)));
                        }
                    } else {
                        agent_ids.push(i as u32);
                        status.push("Ok");
                        round_number.push(Some(round as u32));

                        origins.push(Some(network.stops[journey.origin_stop as usize].name.as_ref()));
                        origin_trip_ids.push(Some(network.get_trip_id(journey.origin_trip)));

                        destinations.push(Some(network.stops[journey.dest_stop as usize].name.as_ref()));
                        destination_trip_ids.push(Some(network.get_trip_id(journey.dest_trip)));

                        journey_times_ms.push(Some(sec_to_milli(journey.duration)));
                        journey_start_times_ms.push(Some(sec_to_milli(journey.start_time)));
                        crowding_costs.push(Some(journey.crowding_cost));
                        num_transfers.push(Some(journey.num_transfers as u32));
                    }
                }
                Err(err) => {
                    agent_ids.push(i as u32);
                    status.push(match err {
                        JourneyError::NoJourneyFound => "No journey found",
                        JourneyError::InfiniteLoop => "Infinite loop",
                    });

                    round_number.push(None);
                    origins.push(None);
                    origin_trip_ids.push(None);

                    destinations.push(None);
                    destination_trip_ids.push(None);

                    journey_times_ms.push(None);
                    journey_start_times_ms.push(None);
                    crowding_costs.push(None);
                    num_transfers.push(None);
                }
            }
        }
    }

    // Set up arrow arrays.

    let agent_ids_arr = Arc::new(UInt32Array::from(agent_ids.clone()));
    let agent_ids_field = Field::new("Agent_Id", agent_ids_arr.data_type().clone(), false);

    let status_arr = Arc::new(StringArray::from(status.clone()));
    let status_field = Field::new("Status", status_arr.data_type().clone(), false);

    let round_number_arr = Arc::new(UInt32Array::from(round_number.clone()));
    let round_number_field = Field::new("Round_Number", round_number_arr.data_type().clone(), true);

    let origins_arr = Arc::new(StringArray::from(origins.clone()));
    let origins_field = Field::new("Origin_Station", origins_arr.data_type().clone(), true);

    let origin_trips_arr = Arc::new(StringArray::from(origin_trip_ids.clone()));
    let origin_trips_field = Field::new("Origin_Trip_ID", origin_trips_arr.data_type().clone(), true);

    let destinations_arr = Arc::new(StringArray::from(destinations.clone()));
    let destination_field = Field::new("Destination_Station", destinations_arr.data_type().clone(), true);

    let destination_trips_arr = Arc::new(StringArray::from(destination_trip_ids.clone()));
    let destination_trips_field = Field::new("Destination_Trip_ID", destination_trips_arr.data_type().clone(), true);

    let journey_durations_arr = Arc::new(Time32MillisecondArray::from(journey_times_ms.clone()));
    let journey_durations_field = Field::new("Journey_Duration", journey_durations_arr.data_type().clone(), true);

    let journey_start_times_arr = Arc::new(Time32MillisecondArray::from(journey_start_times_ms.clone()));
    let journey_start_times_field = Field::new("Journey_Start_Time", journey_start_times_arr.data_type().clone(), true);

    let crowding_costs_arr = Arc::new(Float32Array::from(crowding_costs.clone()));
    let crowding_costs_field = Field::new("Crowding_Cost", crowding_costs_arr.data_type().clone(), true);

    let num_transfers_arr = Arc::new(UInt32Array::from(num_transfers.clone()));
    let num_transfers_field = Field::new("Num_Transfers", num_transfers_arr.data_type().clone(), true);

    let schema = if legs {
        Arc::new(Schema::new(vec![
            agent_ids_field,
            status_field,
            round_number_field,
            origins_field,
            origin_trips_field,
            destination_field,
            journey_durations_field,
            journey_start_times_field,
        ]))
    } else {
        Arc::new(Schema::new(vec![
            agent_ids_field,
            status_field,
            round_number_field,
            origins_field,
            origin_trips_field,
            destination_field,
            destination_trips_field,
            journey_durations_field,
            journey_start_times_field,
            crowding_costs_field,
            num_transfers_field,
        ]))
    };

    let arrays: Vec<ArrayRef> = if legs {
        vec![
            agent_ids_arr,
            status_arr,
            round_number_arr,
            origins_arr,
            origin_trips_arr,
            destinations_arr,
            journey_durations_arr,
            journey_start_times_arr,
        ]
    } else {
        vec![
            agent_ids_arr,
            status_arr,
            round_number_arr,
            origins_arr,
            origin_trips_arr,
            destinations_arr,
            destination_trips_arr,
            journey_durations_arr,
            journey_start_times_arr,
            crowding_costs_arr,
            num_transfers_arr,
        ]
    };

    let record_batch = arrow::record_batch::RecordBatch::try_new(schema, arrays)?;

    // Write to parquet.
    {
        let use_dictionary = true; // TODO: compare with false.
        let props = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            // Because this is a string column with only two possible values, dictionary encoding is useful.
            .set_column_dictionary_enabled("Status".into(), use_dictionary)
            .set_column_dictionary_enabled("Origin_Station".into(), use_dictionary)
            .set_column_dictionary_enabled("Destination_Station".into(), use_dictionary)
            .build();
        let mut writer = ArrowWriter::try_new(writer, record_batch.schema(), Some(props))?;

        writer.write(&record_batch)?;

        writer.close()?;
    }

    Ok(())
}
