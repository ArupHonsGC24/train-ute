use std::path::Path;
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
    println!("{output_path}");
    
    let conn = Connection::open_in_memory()?;
    
    // Grab CSV and extract relevant columns and rows.
    conn.execute(r#"
        CREATE TABLE tbl AS 
        SELECT Business_Date, Station_Name, Arrival_Time_Scheduled, Departure_Time_Scheduled, Passenger_Boardings, Passenger_Alightings 
        FROM read_csv(?, types={'Train_Number': 'VARCHAR', 'Passenger_Boardings': 'Int16', 'Passenger_Alightings': 'Int16'})
        WHERE Mode = 'Metro' AND Passenger_Boardings!=0 OR Passenger_Alightings!=0;
    "#, [csv_filepath])?;
    
    // Export to parquet (couldn't get SQL param to work with output path).
    conn.execute(&format!("COPY tbl TO \'{output_path}\' (FORMAT PARQUET);"), [])?;
    
    Ok(())
}
