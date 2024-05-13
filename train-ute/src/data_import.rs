use std::collections::HashMap;
use std::fs::File;

use arrow::array::{Array, AsArray, BooleanArray, Date32Array, RecordBatch};
use arrow::datatypes::{ArrowPrimitiveType, Date32Type, Time64MicrosecondType, UInt16Type};
use arrow::temporal_conversions::MICROSECONDS;
use chrono::NaiveDate;
use parquet::arrow::arrow_reader::{ArrowPredicate, ParquetRecordBatchReaderBuilder, RowFilter};
use parquet::arrow::ProjectionMask;
use parquet::schema::types::SchemaDescriptor;
use thiserror::Error;

use raptor::Network;

use crate::simulation::{SimulationOp, SimulationStep};

#[derive(Error, Debug)]
pub enum DataImportError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parquet error: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),
    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),
    #[error("No data for date {0}")]
    NoDataForDate(NaiveDate),
}

type Date32TypeNative = <Date32Type as ArrowPrimitiveType>::Native;

// Predicate for filtering records based on a specific date.
struct DateFilterPredicate {
    date: Date32TypeNative,
    column_idx: usize,
    projection_mask: ProjectionMask,
}

impl DateFilterPredicate {
    pub fn new(date: NaiveDate, schema: &SchemaDescriptor) -> Self {
        let date = Date32Type::from_naive_date(date);
        // Find the column index for the "Business_Date" column.
        let column_idx = schema.columns().iter().position(|column| column.name() == "Business_Date").unwrap();
        // Construct the column mask.
        let projection_mask = ProjectionMask::leaves(schema, [column_idx]);
        Self { date, column_idx, projection_mask }
    }
}

impl ArrowPredicate for DateFilterPredicate {
    fn projection(&self) -> &ProjectionMask { &self.projection_mask }

    fn evaluate(&mut self, batch: RecordBatch) -> arrow::error::Result<BooleanArray> {
        // Filter based on date.
        let date_array = batch.column(self.column_idx).as_any().downcast_ref::<Date32Array>().unwrap();
        let filter_mask = BooleanArray::from_unary(date_array, |x| x == self.date);
        Ok(filter_mask)
    }
}

pub fn gen_simulation_steps(path: &str, network: &Network) -> Result<Vec<SimulationStep>, DataImportError> {
    let datafile = File::open(path)?;

    // Use the arrow row filter to only get records for the date we care about.
    let builder = ParquetRecordBatchReaderBuilder::try_new(datafile)?;
    let row_filter = RowFilter::new(vec![Box::new(DateFilterPredicate::new(network.date, builder.parquet_schema()))]);
    let builder = builder.with_row_filter(row_filter);
    let reader = builder.build()?;
    
    let mut station_name_map = HashMap::new();

    let mut simulation_steps = Vec::new();
    for batch in reader {
        // We want to know if the reader returns an error.
        let batch = batch?;

        let station_names = batch.column_by_name("Station_Name").unwrap().as_string::<i32>();
        let passenger_boardings = batch.column_by_name("Passenger_Boardings").unwrap().as_primitive::<UInt16Type>().values();
        let passenger_alightings = batch.column_by_name("Passenger_Alightings").unwrap().as_primitive::<UInt16Type>().values();
        let departure_time_scheduled = batch.column_by_name("Departure_Time_Scheduled").unwrap().as_primitive::<Time64MicrosecondType>();

        for i in 0..batch.num_rows() {
            let station_name = station_names.value(i);
            let stop_idx = if let Some(stop_idx) = station_name_map.get(station_name) {
                *stop_idx
            } else {
                let stop_idx = match network.get_stop_idx_from_name(&station_name) {
                    Some(idx) => idx,
                    None => {
                        eprintln!("Station not found: {}", station_name);
                        continue;
                    }
                };
                station_name_map.insert(station_name.to_string(), stop_idx);
                stop_idx
            };

            let time = (departure_time_scheduled.value(i) / MICROSECONDS) as u32;
            let boardings = passenger_boardings[i];
            let alightings = passenger_alightings[i];

            if boardings > 0 {
                simulation_steps.push(SimulationStep {
                    time,
                    op: SimulationOp::SpawnAgents {
                        stop_idx,
                        count: boardings,
                    },
                });
            }
            if alightings > 0 {
                simulation_steps.push(SimulationStep {
                    time,
                    op: SimulationOp::DeleteAgents {
                        stop_idx,
                        count: alightings,
                    },
                });
            }
        }
    }

    if simulation_steps.len() == 0 {
        Err(DataImportError::NoDataForDate(network.date))
    } else {
        Ok(simulation_steps)
    }
}