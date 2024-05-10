use std::path::Path;
use std::time::Instant;
use duckdb::{Connection, Result};

fn main() -> Result<()> {
    let csv_filepath = loop {
        println!("Input data file path:");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        input = input.replace("\"", "").trim().to_owned();
        
        if Path::new(&input).exists() {
            break input;
        } else {
            println!("File does not exist. Please try again.");
        }
    };
    
    let mut output_path = Path::new(&csv_filepath).to_path_buf();
    output_path.set_extension("parquet");
    let output_path = format!("{}", output_path.to_str().unwrap());
    println!("\nOutput: {output_path}");
    
    let conn = Connection::open_in_memory()?;

    let execute_start = Instant::now();
    
    // Grab CSV and extract relevant columns and rows.
    conn.execute(r#"
        CREATE TABLE tbl AS 
        SELECT Business_Date, Station_Name, Arrival_Time_Scheduled, Departure_Time_Scheduled, Passenger_Boardings, Passenger_Alightings 
        FROM read_csv(?, types={
            'Passenger_Boardings': 'UInt16',
            'Passenger_Alightings': 'UInt16'
        })
        WHERE Mode = 'Metro' AND (Passenger_Boardings != 0 OR Passenger_Alightings != 0);
    "#, [csv_filepath])?;

    let execute_end = Instant::now();
    
    // Export to parquet (couldn't get SQL param to work with output path).
    conn.execute(&format!("COPY tbl TO \'{output_path}\' (FORMAT PARQUET);"), [])?;

    let export_end = Instant::now();

    println!("Execute: {:?}, Export: {:?}", execute_end - execute_start, export_end - execute_end);
    
    Ok(())
}
