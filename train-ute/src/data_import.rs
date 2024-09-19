use crate::simulation::{AgentCount, SimulationStep};
use arrow::array::AsArray;
use arrow::datatypes::{Int32Type, Time64MicrosecondType};
use chrono::NaiveDate;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::file::reader::ChunkReader;
use raptor::network::{StopIndex, Timestamp};
use raptor::Network;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DataImportError {
    #[error("Parquet error: {0}.")]
    Parquet(#[from] parquet::errors::ParquetError),
    #[error("Arrow error: {0}.")]
    Arrow(#[from] arrow::error::ArrowError),
    #[error("No data for date {0}.")]
    NoDataForDate(NaiveDate),
    #[error("Column not found: {0}.")]
    ColumnNotFound(&'static str),
    #[error("Column {0} wrong format: wanted {1}.")]
    ColumnWrongFormat(&'static str, &'static str),
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
            Some(*stop_idx)
        } else {
            let stop_idx = network.get_stop_idx_from_name(&station_name)?;
            station_name_map.insert(station_name.to_string(), stop_idx);
            Some(stop_idx)
        }
    };

    let mut simulation_steps = Vec::new();
    for batch in reader {
        // We want to know if the reader returns an error.
        let batch = batch?;

        let origins = batch.column_by_name("Origin_Station")
                           .ok_or(DataImportError::ColumnNotFound("Origin_Station"))?
            .as_string_opt::<i32>()
            .ok_or(DataImportError::ColumnWrongFormat("Departure_Time", "String"))?;
        let destinations = batch.column_by_name("Destination_Station")
                                .ok_or(DataImportError::ColumnNotFound("Destination_Station"))?
            .as_string_opt::<i32>()
            .ok_or(DataImportError::ColumnWrongFormat("Departure_Time", "String"))?;
        let departure_times_us = batch.column_by_name("Departure_Time")
                                   .ok_or(DataImportError::ColumnNotFound("Departure_Time"))?
            .as_primitive_opt::<Time64MicrosecondType>()
            .ok_or(DataImportError::ColumnWrongFormat("Departure_Time", "Time64Microsecond"))?;
        let num_agents = batch.column_by_name("Agent_Count")
                              .ok_or(DataImportError::ColumnNotFound("Agent_Count"))?
            .as_primitive_opt::<Int32Type>()
            .ok_or(DataImportError::ColumnWrongFormat("Agent_Count", "Int32"))?
            .values();

        for i in 0..batch.num_rows() {
            let origin_name = origins.value(i);
            let Some(origin_stop) = get_stop_idx_from_name(&network, origin_name) else {
                // TODO: alert user first time?
                eprintln!("Station not found: {origin_name}");
                continue;
            };
            let dest_name = destinations.value(i);
            let Some(dest_stop) = get_stop_idx_from_name(&network, dest_name) else {
                // TODO: alert user.
                eprintln!("Station not found: {dest_name}");
                continue;
            };

            // Convert from microseconds to seconds.
            let departure_time = (departure_times_us.value(i) / 1_000_000 ) as Timestamp;
            let count = num_agents[i] as AgentCount;

            simulation_steps.push(SimulationStep {
                departure_time,
                origin_stop,
                dest_stop,
                count,
            });
        }
    }

    if simulation_steps.len() == 0 {
        Err(DataImportError::NoDataForDate(network.date))
    } else {
        Ok(simulation_steps)
    }
}
