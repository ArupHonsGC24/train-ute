use std::fs::File;
use std::ops::Deref;
use std::sync::Arc;
use arrow::array::{Array, FixedSizeListBuilder, StringBuilder, UInt8Builder};
use arrow::datatypes::{Field, Schema};
use geoarrow::array::{CoordType, LineStringBuilder};
use geoarrow::datatypes::GeoDataType;
use geoarrow::GeometryArrayTrait;
use gtfs_structures::Gtfs;
use thiserror::Error;
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
        num_shapes
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
    let mut tbl = geoarrow::table::GeoTable::from_arrow(vec![record_batch], schema, None, Some(GeoDataType::LineString(CoordType::Interleaved)))?;

    let mut shape_file = File::create(path)?;
    Ok(geoarrow::io::ipc::write_ipc(&mut tbl, &mut shape_file)?)
}

pub fn export_agent_transfers(path: &str, agent_transfers: &[AgentTransfer]) -> Result<(), DataExportError> {



    Ok(())
}