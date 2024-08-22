use std::fs::File;
use std::io::Write;
use std::mem;
use std::path::Path;
use std::sync::Arc;

use arrow::array::{Array, StringArray, TimestampMillisecondArray, UInt32Array};
use arrow::datatypes::{Field, Schema};
use itertools::{Itertools, izip};
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use rgb::RGB8;
use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use raptor::Network;
use raptor::network::{CoordType, NetworkPoint, Timestamp};
use raptor::utils::get_time_str;

use crate::simulation::SimulationResult;
use crate::utils::{mix_rgb, quadratic_ease_in_out, quadratic_inv_ease_in_out};

#[derive(Error, Debug)]
pub enum DataExportError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Arrow error: {0}")]
    ArrowError(#[from] arrow::error::ArrowError),
    #[error("Parquet error: {0}")]
    ParquetError(#[from] parquet::errors::ParquetError),
    #[error("CSV error: {0}")]
    CsvError(#[from] csv::Error),
}

// Writes a set of binary data to a zip file in a simple format:
// - A 32-bit byte offset and length for each data chunk.
// - The binary data chunks, each aligned to 8 bytes.
fn write_bin(path: &Path, data_list: &[&[u8]]) -> std::io::Result<()> {
    // Simple power-of-two alignment.
    fn round_up_to_eight(num: usize) -> usize { (num + 7) & !7 }

    // Open zip file.
    let mut zip = ZipWriter::new(File::create(path)?);
    zip.start_file("data.bin", SimpleFileOptions::default())?;

    // A 32-bit byte offset and length for each data chunk, followed by the data chunks.
    // We want the data to be aligned to 8 bytes.
    let header_size = data_list.len() * 2 * mem::size_of::<u32>(); // 2 32-bit values per data chunk.
    let mut index = header_size as u32; // Start past header.
    let mut written_bytes = 0;
    for &data in data_list {
        written_bytes += zip.write(&index.to_le_bytes())?;
        written_bytes += zip.write(&(data.len() as u32).to_le_bytes())?;
        index += round_up_to_eight(data.len()) as u32;
    }

    // Sanity check.
    assert_eq!(written_bytes, header_size);

    // Write data, maintaining 8-byte alignment.
    for &data in data_list {
        zip.write_all(data)?;
        let padding = round_up_to_eight(data.len()) - data.len();
        for _ in 0..padding {
            zip.write_all(&0u8.to_le_bytes())?;
        }
    }

    Ok(())
}

pub fn export_shape_file(path: &Path, network: &Network) -> Result<(), DataExportError> {
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

    write_bin(path, &[bytemuck::must_cast_slice(&shape_points), bytemuck::must_cast_slice(&shape_start_indices), &shape_colours])?;

    Ok(())
}

pub fn export_network_trips(path: &Path, network: &Network, simulation_result: &SimulationResult) -> Result<(), DataExportError> {
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

            let agent_counts = &simulation_result.agent_journeys[route.get_trip_range(trip_idx)];

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
                        println!("Warning: Shape index out of bounds for route {}, stop {}({arr_stop_order}).", network.routes[route_idx].line, network.stops[arr_stop_idx].name);
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
                    assert!(distance_along_shape_section > 0.);
                    
                    // Calculate proportion along this shape we are, for interpolating properties.
                    // Apply an easing function to the proportion, so trains accelerate and decelerate.
                    // We use the inverse of the easing function for easing time.
                    let proportion = (distance / distance_along_shape_section) as f32;
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

    write_bin(path, &[bytemuck::must_cast_slice(&trip_points), bytemuck::must_cast_slice(&start_indices), bytemuck::must_cast_slice(&trip_times), &trip_colours])?;

    Ok(())
}

// Exports the agent counts to a parquet (and csv) file.
pub fn export_agent_counts(path: &Path, network: &Network, simulation_result: &SimulationResult) -> Result<(), DataExportError> {
    // This is the utc timestamp for the midnight of the day the network represents.
    let date_timestamp = network.date.and_time(chrono::NaiveTime::MIN).and_utc().timestamp();

    let mut trip_ids = Vec::new();
    let mut timestamps = Vec::new(); // Unix timestamps in milliseconds.
    let mut departures = Vec::new();
    let mut departure_ids = Vec::new();
    let mut arrivals = Vec::new();
    let mut arrival_ids = Vec::new();
    let mut agent_counts = Vec::new();

    for route in network.routes.iter() {
        for trip in 0..route.num_trips as usize {
            let trip_id = route.trip_ids[trip].as_ref();
            let trip_range = route.get_trip_range(trip);

            let stop_times = network.stop_times[trip_range.clone()].iter().map(|stop_time|{
                (date_timestamp + stop_time.departure_time as i64) * 1000 // Convert to milliseconds, as seconds is not as widely supported.
            });
            let stops = route.get_stops(&network.route_stops).iter().tuple_windows();
            let trip_agent_counts = &simulation_result.agent_journeys[trip_range.clone()];

            for ((&dep_stop_idx, &arr_stop_idx), time, &agent_count) in izip!(stops, stop_times, trip_agent_counts) {
                trip_ids.push(trip_id);
                timestamps.push(time);
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

    let schema = Arc::new(Schema::new(vec![trip_id_field, timestamp_field, departures_field, departure_ids_field, arrivals_field, arrival_ids_field, agent_counts_field]));
    // TODO: A record batch per trip? Sort trips by earliest departure time?
    let record_batch = arrow::record_batch::RecordBatch::try_new(schema, vec![trip_id_arr, timestamps_arr, departures_arr, departure_ids_arr, arrivals_arr, arrival_ids_arr, agent_counts_arr])?;

    // Write to parquet.
    {
        let props = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .build();
        let mut writer = parquet::arrow::ArrowWriter::try_new(File::create(path.with_extension("parquet"))?, record_batch.schema(), Some(props))?;

        writer.write(&record_batch)?;

        writer.close()?;
    }
    
    // Write to csv (for debugging).
    {
        let mut csv_writer = csv::Writer::from_path(path.with_extension("csv"))?;
        csv_writer.write_record(&["trip_id", "timestamp", "departure", "departure_id", "arrival", "arrival_id", "count"])?;
        let date_str = network.date.to_string();
        for (trip_name, timestamp, departure, departure_id, arrival, arrival_id, count) in izip!(trip_ids, timestamps, departures, departure_ids, arrivals, arrival_ids, agent_counts) {
            let timestamp = format!("{date_str} {}", &get_time_str((timestamp/1000 - date_timestamp) as Timestamp));
            csv_writer.write_record(&[trip_name, &timestamp, departure, departure_id, arrival, arrival_id, &count.to_string()])?;
        }
    }
    
    Ok(())
}

pub fn export_stops(path: &Path, network: &Network) -> Result<(), DataExportError> {
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
