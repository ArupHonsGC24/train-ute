use crate::simulation::{AgentCount, PopulationCount, SimulationStep, TripCapacity};
use arrow::array::AsArray;
use arrow::datatypes::{Int64Type, Time64NanosecondType};
use chrono::NaiveDate;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::file::reader::ChunkReader;
use raptor::network::{StopIndex, Timestamp};
use raptor::Network;
use std::collections::HashMap;
use std::io::Read;
use itertools::Itertools;

#[derive(thiserror::Error, Debug)]
pub enum DataImportError {
    #[error("Parquet error: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),
    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),
    #[error("No data for date {0}")]
    NoDataForDate(NaiveDate),
    #[error("Header not found: {0}")]
    HeaderNotFound(&'static str),
    #[error("Column not found: {0}")]
    ColumnNotFound(&'static str),
    #[error("Column {0} wrong format: wanted {1}")]
    ColumnWrongFormat(&'static str, &'static str),
    #[error("No data found")]
    NoData,
}

//type Date32TypeNative = <Date32Type as ArrowPrimitiveType>::Native;
//
//// Predicate for filtering records based on a specific date.
//struct DateFilterPredicate {
//    date: Date32TypeNative,
//    column_idx: usize,
//    projection_mask: ProjectionMask,
//}
//
//impl DateFilterPredicate {
//    pub fn new(date: NaiveDate, schema: &SchemaDescriptor) -> Self {
//        let date = Date32Type::from_naive_date(date);
//        // Find the column index for the "Business_Date" column.
//        let column_idx = schema.columns().iter().position(|column| column.name() == "Business_Date").unwrap();
//        // Construct the column mask.
//        let projection_mask = ProjectionMask::leaves(schema, [column_idx]);
//        Self { date, column_idx, projection_mask }
//    }
//}
//
//impl ArrowPredicate for DateFilterPredicate {
//    fn projection(&self) -> &ProjectionMask { &self.projection_mask }
//
//    fn evaluate(&mut self, batch: RecordBatch) -> arrow::error::Result<BooleanArray> {
//        // Filter based on date.
//        let date_array = batch.column(self.column_idx).as_any().downcast_ref::<Date32Array>().unwrap();
//        let filter_mask = BooleanArray::from_unary(date_array, |x| x == self.date);
//        Ok(filter_mask)
//    }
//}

//pub fn build_simulation_steps_from_patronage_data(path: &str, network: &Network) -> Result<Vec<SimulationStep>, DataImportError> {
pub fn build_simulation_steps_from_patronage_data(reader: impl ChunkReader + 'static, network: &Network) -> Result<Vec<SimulationStep>, DataImportError> {
    let builder = ParquetRecordBatchReaderBuilder::try_new(reader)?;

    // Use the arrow row filter to only get records for the date we care about.
    //let row_filter = RowFilter::new(vec![Box::new(DateFilterPredicate::new(network.date, builder.parquet_schema()))]);
    //let builder = builder.with_row_filter(row_filter);

    let reader = builder.build()?;

    // Hashmap used to cache stop indices.
    let mut station_name_map = HashMap::new();
    let mut get_stop_idx_from_name = |network: &Network, station_name: &str| -> Option<StopIndex> {
        if let Some(stop_idx) = station_name_map.get(station_name) {
            *stop_idx
        } else {
            let stop_idx = network.get_stop_idx_from_name(&station_name);
            if stop_idx.is_none() {
                log::warn!("Station not found: {station_name}");
            }
            station_name_map.insert(station_name.to_string(), stop_idx);
            stop_idx
        }
    };

    let mut simulation_steps = HashMap::new();
    for batch in reader {
        // We want to know if the reader returns an error.
        let batch = batch?;

        // TODO: This could accept more different types (different int, string and time types).
        let origins = batch.column_by_name("Origin_Station")
                           .ok_or(DataImportError::ColumnNotFound("Origin_Station"))?
            .as_string_opt::<i64>()
            .ok_or(DataImportError::ColumnWrongFormat("Origin_Station", "String"))?;
        let destinations = batch.column_by_name("Destination_Station")
                                .ok_or(DataImportError::ColumnNotFound("Destination_Station"))?
            .as_string_opt::<i64>()
            .ok_or(DataImportError::ColumnWrongFormat("Destination_Station", "String"))?;
        let departure_times_ns = batch.column_by_name("Departure_Time")
                                      .ok_or(DataImportError::ColumnNotFound("Departure_Time"))?
            .as_primitive_opt::<Time64NanosecondType>()
            .ok_or(DataImportError::ColumnWrongFormat("Departure_Time", "Time64Nanosecond"))?;
        let num_agents = batch.column_by_name("Agent_Count")
                              .ok_or(DataImportError::ColumnNotFound("Agent_Count"))?
            .as_primitive_opt::<Int64Type>()
            .ok_or(DataImportError::ColumnWrongFormat("Agent_Count", "Int64"))?
            .values();

        for i in 0..batch.num_rows() {
            let origin_name = origins.value(i);
            let Some(origin_stop) = get_stop_idx_from_name(&network, origin_name) else {
                continue;
            };
            let dest_name = destinations.value(i);
            let Some(dest_stop) = get_stop_idx_from_name(&network, dest_name) else {
                continue;
            };

            // Convert from nanoseconds to seconds.
            let departure_time = (departure_times_ns.value(i) / 1_000_000_000) as Timestamp;
            let count = num_agents[i] as AgentCount;

            let simulation_step = simulation_steps.entry((departure_time, origin_stop))
                                                  .or_insert_with(|| SimulationStep::new(departure_time, origin_stop));
            simulation_step.push(dest_stop, count);
        }
    }

    if simulation_steps.len() == 0 {
        Err(DataImportError::NoDataForDate(network.date))
    } else {
        Ok(simulation_steps.into_values().collect_vec())
    }
}

pub fn import_trip_capacities(reader: impl Read) -> Result<HashMap<String, TripCapacity>, DataImportError> {
    let mut csv_reader = csv::Reader::from_reader(reader);
    let headers = csv_reader.headers().map_err(|_| DataImportError::ColumnNotFound("header"))?;

    if headers.get(0) != Some("trip_id") {
        return Err(DataImportError::ColumnNotFound("trip_id"));
    }
    if headers.get(1) != Some("seated") {
        return Err(DataImportError::ColumnNotFound("seated"));
    }
    if headers.get(2) != Some("standing") {
        return Err(DataImportError::ColumnNotFound("standing"));
    }

    let capacities: HashMap<_, _> = csv_reader.into_records().filter_map(|record| {
        let record = record.ok()?;
        // trip_id,seated,standing
        let trip_id = record.get(0)?;
        let seated = record.get(1)?.parse::<PopulationCount>().ok()?;
        let standing = record.get(2)?.parse::<PopulationCount>().ok()?;
        Some((trip_id.to_string(), TripCapacity { seated, standing }))
    }).collect();

    if capacities.is_empty() {
        Err(DataImportError::NoData)
    } else {
        Ok(capacities)
    }
}