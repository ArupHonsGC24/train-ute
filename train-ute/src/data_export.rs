use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::mem;

use gtfs_structures::Gtfs;
use thiserror::Error;

use raptor::Network;

use crate::simulation::AgentTransfer;

#[derive(Error, Debug)]
pub enum DataExportError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

// Writes a set of binary data to a file in a simple format:
// - A 32-bit byte offset and length for each data chunk.
// - The binary data chunks, each aligned to 8 bytes.
pub fn write_bin(path: &str, data_list: &[&[u8]]) -> std::io::Result<()> {
    fn round_up_to_eight(num: usize) -> usize {
        (num + 7) & !7
    }

    let mut output_file = File::create(path)?;

    // A 32-bit byte offset and length for each data chunk, followed by the data chunks.
    // We want the data to be aligned to 8 bytes.
    let header_size = data_list.len() * 2 * mem::size_of::<u32>(); // 2 32-bit values per data chunk.
    let mut index = header_size as u32; // Start past header.
    let mut written_bytes = 0;
    for &data in data_list {
        written_bytes += output_file.write(&index.to_le_bytes())?;
        written_bytes += output_file.write(&(data.len() as u32).to_le_bytes())?;
        index += round_up_to_eight(data.len()) as u32;
    }

    // Sanity check.
    assert_eq!(written_bytes, header_size);

    // Write data, maintaining 8-byte alignment.
    for &data in data_list {
        output_file.write_all(data)?;
        let padding = round_up_to_eight(data.len()) - data.len();
        for _ in 0..padding {
            output_file.write_all(&0u8.to_le_bytes())?;
        }
    }

    Ok(())
}

pub fn export_shape_file(path: &str, gtfs: &Gtfs) -> Result<(), DataExportError> {
    // TODO: Filter shapes.
    let mut shape_points = Vec::new();
    let mut shape_start_indices = Vec::new();
    let mut shape_colours = Vec::new();

    let mut colour_to_height_map = HashMap::new();
    let mut last_height = 0f32;

    for (shape_id, shape) in gtfs.shapes.iter() {
        // Find the colour of the line by looking up the first trip that uses the shape, then the route of that trip.
        let trip = gtfs.trips.values().find(|trip| trip.shape_id.as_ref() == Some(shape_id)).unwrap();
        let colour = gtfs.routes.get(&trip.route_id).unwrap().color;

        // Determine height based on colour
        let height = if let Some(&height) = colour_to_height_map.get(&colour) {
            height
        } else {
            last_height += 10.;
            colour_to_height_map.insert(colour, last_height);
            last_height
        };

        // Indices are based on points, not coordinates.
        shape_start_indices.push(shape_points.len() as u32 / 3);

        // Construct line string from shape.
        for point in shape {
            shape_points.push(point.longitude as f32);
            shape_points.push(point.latitude as f32);
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

    let height = 100f32;

    let mut start_indices = Vec::new();
    let mut trip_points = Vec::new();
    let mut trip_times = Vec::new();
    let mut trip_colours = Vec::new();

    for route in 0..network.num_routes() {
        let num_stops = network.num_stops_in_route(route);
        for trip in 0..network.num_trips(route) {
            start_indices.push(trip_points.len() as u32 / 3);

            for stop_order in 0..num_stops {
                // Push location.
                let stop_idx = network.get_stop_in_route(route, stop_order);

                trip_points.push(network.stop_points[stop_idx as usize].0);
                trip_points.push(network.stop_points[stop_idx as usize].1);
                trip_points.push(height);

                // Push time.
                let arrival_time = network.get_arrival_time(route, trip, stop_order);
                trip_times.push(arrival_time as f32);

                // Push colour.
                let route_colour = network.routes[route].color;
                trip_colours.push(route_colour.0);
                trip_colours.push(route_colour.1);
                trip_colours.push(route_colour.2);
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
        points.push(start.0);
        points.push(start.1);
        points.push(height);

        let end = network.stop_points[transfer.end_idx as usize];
        points.push(end.0);
        points.push(end.1);
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
        }
    }

    write_bin(path, &[bytemuck::must_cast_slice(&points), bytemuck::must_cast_slice(&start_indices), bytemuck::must_cast_slice(&timestamps), &colours])?;

    Ok(())
}
