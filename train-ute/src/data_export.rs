use std::fs::File;
use std::io::Write;
use std::mem;

use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use raptor::Network;
use raptor::network::NetworkPoint;

use crate::simulation::AgentTransfer;

#[derive(Error, Debug)]
pub enum DataExportError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

// Writes a set of binary data to a zip file in a simple format:
// - A 32-bit byte offset and length for each data chunk.
// - The binary data chunks, each aligned to 8 bytes.
fn write_bin(path: &str, data_list: &[&[u8]]) -> std::io::Result<()> {
    fn round_up_to_eight(num: usize) -> usize {
        (num + 7) & !7
    }

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

// Simple inverse quadratic easing.
fn quadratic_inv_ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        (t * 0.5).sqrt()
    } else {
        1. - ((1. - t) * 0.5).sqrt()
    }
}

pub fn export_shape_file(path: &str, network: &Network) -> Result<(), DataExportError> {
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

pub fn export_network_trips(path: &str, network: &Network) -> Result<(), DataExportError> {
    const NUM_COORDS_PER_POINT: u32 = 3;

    // I haven't bothered to calculate capacities, but it's amortised constant to push anyway so there's not really any point.
    let mut start_indices = Vec::new();
    let mut trip_points = Vec::new();
    let mut trip_times = Vec::new();
    let mut trip_colours = Vec::new();

    for route_idx in 0..network.num_routes() {
        let num_stops = network.num_stops_in_route(route_idx);
        let route_colour = network.routes[route_idx].colour;
        let route_shape = &network.routes[route_idx].shape;
        let height = network.routes[route_idx].shape_height;

        for trip_idx in 0..network.num_trips(route_idx) {
            start_indices.push(trip_points.len() as u32 / NUM_COORDS_PER_POINT);

            let mut shape_idx = 0;
            for dep_stop_order in 0..num_stops - 1 {
                let arr_stop_order = dep_stop_order + 1;

                let departure_time = network.get_departure_time(route_idx, trip_idx, dep_stop_order) as f32;

                let arr_stop_idx = network.get_stop_in_route(route_idx, arr_stop_order) as usize;
                let arr_point = network.stop_points[arr_stop_idx];
                let arrival_time = network.get_arrival_time(route_idx, trip_idx, arr_stop_order) as f32;

                let mut push_point = |point: NetworkPoint, next_point: NetworkPoint| {
                    // Location is offset to the left to separate inbound and outbound.
                    const OFFSET: f32 = 20.;
                    let offset_point = point.left_offset(next_point, OFFSET);
                    trip_points.push(offset_point.longitude);
                    trip_points.push(offset_point.latitude);
                    trip_points.push(height);

                    // Colour (RGBA)
                    trip_colours.push(route_colour.r);
                    trip_colours.push(route_colour.g);
                    trip_colours.push(route_colour.b);
                    trip_colours.push(255);
                };

                // Go through shape points and push.
                let start_shape_idx = shape_idx;
                let mut current_point = route_shape[shape_idx];
                let mut distance_along_shape_section = 0f32;
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

                // Calculate time assuming constant speed.
                let section_duration = arrival_time - departure_time;
                let mut distance = 0.;
                for shape_idx in start_shape_idx..shape_idx {
                    assert!(distance >= 0.);
                    assert!(distance_along_shape_section > 0.);
                    // Apply an easing function to the proportion, so trains accelerate and decelerate.
                    // We use the inverse of the easing function for easing time.
                    let proportion = quadratic_inv_ease_in_out(distance / distance_along_shape_section);
                    let time = departure_time + section_duration * proportion;
                    trip_times.push(time);
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

pub fn export_agent_transfers(path: &str, network: &Network, agent_transfers: &[AgentTransfer]) -> Result<(), DataExportError> {
    // A path list of 2-point paths representing transfers.
    let num_transfers = agent_transfers.len();

    let mut start_indices = Vec::with_capacity(num_transfers);
    let mut points = Vec::with_capacity(num_transfers * 6);
    let mut timestamps = Vec::with_capacity(num_transfers * 2);
    let mut colours = Vec::with_capacity(num_transfers * 6);

    let height = 100.;

    for transfer in agent_transfers {
        start_indices.push(points.len() as u32 / 3);

        // Push the start and end points.
        let start = network.stop_points[transfer.start_idx as usize];
        points.push(start.longitude);
        points.push(start.latitude);
        points.push(height);

        let end = network.stop_points[transfer.end_idx as usize];
        points.push(end.longitude);
        points.push(end.latitude);
        points.push(height);

        // Push the timestamps.
        timestamps.push(transfer.timestamp as f32);
        timestamps.push(transfer.arrival_time as f32);

        // Push the colours. TODO: Colour in and outbound.
        // Purple for now.
        for _ in 0..2 {
            colours.push(0xA0u8);
            colours.push(0x20u8);
            colours.push(0xF0u8);
            colours.push(0xFFu8);
        }
    }

    write_bin(path, &[bytemuck::must_cast_slice(&points), bytemuck::must_cast_slice(&start_indices), bytemuck::must_cast_slice(&timestamps), &colours])?;

    Ok(())
}
