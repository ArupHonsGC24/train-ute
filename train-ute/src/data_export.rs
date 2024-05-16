use std::fs::File;
use std::ops::Deref;
use std::sync::Arc;

use arrow::array::{Array, FixedSizeListBuilder, Float64Array, StringBuilder, Time32SecondArray, TimestampSecondArray, UInt16Array, UInt8Builder};
use arrow::datatypes::{Field, Schema, TimestampSecondType};
use geoarrow::array::{CoordType, LineStringBuilder, PointBuilder};
use geoarrow::datatypes::GeoDataType;
use geoarrow::GeometryArrayTrait;
use geoarrow::table::GeoTable;
use gtfs_structures::Gtfs;
use thiserror::Error;

use raptor::Network;

use crate::simulation::AgentTransfer;

#[derive(Error, Debug)]
pub enum DataExportError {
    #[error("Arrow error: {0}")]
    ArrowError(#[from] arrow::error::ArrowError),
    #[error("GeoArrow error: {0}")]
    GeoArrowError(#[from] geoarrow::error::GeoArrowError),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub fn export_shape_file(path: &str, gtfs: &Gtfs) -> Result<(), DataExportError> {
    let num_shapes = gtfs.shapes.len();
    let mut line_strings = Vec::with_capacity(num_shapes);
    let mut line_colours = FixedSizeListBuilder::with_capacity(
        UInt8Builder::new(),
        3,
        num_shapes,
    );
    let mut line_colours_hex = StringBuilder::with_capacity(num_shapes, num_shapes * 7);
    for (shape_id, shape) in gtfs.shapes.iter() {
        // Construct line string from shape.
        let mut line_string = Vec::with_capacity(shape.len());
        for point in shape {
            line_string.push(geo_types::Coord {
                x: point.longitude,
                y: point.latitude,
            });
        }
        line_strings.push(geo_types::LineString::new(line_string));

        // Find the colour of the line by looking up the first trip that uses the shape, then the route of that trip.

        let trip = gtfs.trips.values().find(|trip| trip.shape_id.as_ref() == Some(shape_id)).unwrap();
        let colour = gtfs.routes.get(&trip.route_id).unwrap().color;

        line_colours.values().append_value(colour.r);
        line_colours.values().append_value(colour.g);
        line_colours.values().append_value(colour.b);
        line_colours.append(true);

        line_colours_hex.append_value(&format!("#{:02X}{:02X}{:02X}", colour.r, colour.g, colour.b));
    }

    let shape_arr = LineStringBuilder::<i32>::from_line_strings(
        &line_strings,
        Some(CoordType::Interleaved),
        Default::default(),
    ).finish();

    let line_colour_arr = Arc::new(line_colours.finish());
    let line_colour_arr_hex = Arc::new(line_colours_hex.finish());

    let colour_field = Field::new("colour", line_colour_arr.data_type().clone(), false);
    let colour_field_hex = Field::new("colour_hex", line_colour_arr_hex.data_type().clone(), false);
    let schema = Arc::new(Schema::new(vec![shape_arr.extension_field().deref().clone(), colour_field, colour_field_hex]));

    let record_batch = arrow::record_batch::RecordBatch::try_new(schema.clone(), vec![shape_arr.into_array_ref(), line_colour_arr, line_colour_arr_hex])?;
    let mut tbl = GeoTable::from_arrow(vec![record_batch], schema, None, Some(GeoDataType::LineString(CoordType::Interleaved)))?;

    let mut output_file = File::create(path)?;
    Ok(geoarrow::io::ipc::write_ipc(&mut tbl, &mut output_file)?)
}

pub fn export_agent_transfers(path: &str, gtfs: &Gtfs, network: &Network, agent_transfers: &[AgentTransfer]) -> Result<(), DataExportError> {
    // Precalculate stop points.
    let mut stop_points = Vec::with_capacity(network.num_stops());
    for stop_idx in 0..network.num_stops() {
        let stop_id = network.get_stop(stop_idx).id.as_ref();
        let stop = &gtfs.stops[stop_id];
        stop_points.push(geo_types::Point(geo_types::Coord {
            x: stop.longitude.unwrap(),
            y: stop.latitude.unwrap(),
        }));
    }

    // Convert to columnar format.
    let date_timestamp = network.date.and_time(chrono::NaiveTime::MIN).and_utc().timestamp();
    let timestamps_arr: Arc<TimestampSecondArray> = Arc::new(agent_transfers.iter().map(|x| date_timestamp + x.timestamp as i64).collect::<Vec<_>>().into());
    let timestamp_field = Field::new("timestamp", timestamps_arr.data_type().clone(), false);
    let agent_counts_arr: Arc<UInt16Array> = Arc::new(agent_transfers.iter().map(|x| x.count).collect());
    let agent_counts_field = Field::new("count", agent_counts_arr.data_type().clone(), false);
    
    let latitudes: Arc<Float64Array> = Arc::new(agent_transfers.iter().map(|x| stop_points[x.start_idx as usize].0.y).collect());
    let latitudes_field = Field::new("latitude", latitudes.data_type().clone(), false);
    let longitudes: Arc<Float64Array> = Arc::new(agent_transfers.iter().map(|x| stop_points[x.start_idx as usize].0.x).collect());
    let longitudes_field = Field::new("longitude", longitudes.data_type().clone(), false);
    
    let points = agent_transfers.iter().map(|x| stop_points[x.start_idx as usize]).collect::<Vec<_>>();
    let point_arr = PointBuilder::from_points(points.iter(), Some(CoordType::Interleaved), Default::default()).finish();

    let schema = Arc::new(Schema::new(vec![timestamp_field, agent_counts_field, point_arr.extension_field().deref().clone()]));
    //let schema = Arc::new(Schema::new(vec![timestamp_field, agent_counts_field, latitudes_field, longitudes_field]));

    let record_batch = arrow::record_batch::RecordBatch::try_new(schema.clone(), vec![timestamps_arr, agent_counts_arr, point_arr.into_array_ref()])?;
    //let record_batch = arrow::record_batch::RecordBatch::try_new(schema.clone(), vec![timestamps_arr, agent_counts_arr, latitudes, longitudes])?;
    let mut tbl = GeoTable::from_arrow(vec![record_batch], schema.clone(), None, Some(GeoDataType::Point(CoordType::Interleaved)))?;
    
    let mut output_file = File::create(path)?;
    
    // let mut filewriter = arrow::ipc::writer::FileWriter::try_new(output_file, &schema)?;
    // filewriter.write(&record_batch)?;
    // filewriter.finish()?;
    
    geoarrow::io::parquet::write_geoparquet(&mut tbl, &mut output_file, None)?;
    
    //geoarrow::io::csv::write_csv(&mut tbl, &mut output_file)?;
    //geoarrow::io::geojson::write_geojson(&mut tbl, &mut output_file)?;
    //geoarrow::io::ipc::write_ipc(&mut tbl, &mut output_file)?;
    
    Ok(())
}