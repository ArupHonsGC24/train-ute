use std::io::Cursor;
use std::sync::Mutex;

use chrono::NaiveDate;
use gtfs_structures::{Gtfs, GtfsReader};
use tauri::{ipc, State};

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Unexpected request body.")]
    RequestBodyMustBeRaw,
/*    #[error("Missing header entry: `{0}`.")]
    MissingHeader(&'static str),
    #[error("Malformed header entry: `{0}`.")]
    MalformedHeaderEntry(&'static str),*/
    #[error("IO error: {0}.")]
    Io(#[from] std::io::Error),
    #[error("GTFS error: {0}.")]
    Gtfs(#[from] gtfs_structures::Error),
}

impl serde::Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}

#[derive(serde::Serialize, Clone)]
struct DateRange {
    min: NaiveDate,
    max: NaiveDate,
}

#[derive(Default)]
struct AppStateData {
    gtfs: Option<Gtfs>,
    date_range: Option<DateRange>,
}

#[derive(Default)]
struct AppState {
    data: Mutex<AppStateData>,
}

#[tauri::command]
async fn load_gtfs(request: ipc::Request<'_>, state: State<'_, AppState>) -> Result<DateRange, Error> {
    let ipc::InvokeBody::Raw(gtfs_zip) = request.body() else {
        return Err(Error::RequestBodyMustBeRaw);
    };

    // Load GTFS data. TODO: Why does this take so long? Probably because it was a debug build.
    // TODO: Refactor this into a separate function (and put in a separate frontend crate?).
    match GtfsReader::default().raw().read_from_reader(Cursor::new(gtfs_zip)).and_then(Gtfs::try_from) {
        Ok(gtfs) => {
            println!("Successfully loaded GTFS data in {}ms.", gtfs.read_duration);
            let mut app_data = state.data.lock().unwrap();
            let date_range = DateRange {
                // TODO: Look at calendar dates too.
                min: gtfs.calendar.values().map(|c| c.start_date).min().unwrap(),
                max: gtfs.calendar.values().map(|c| c.end_date).max().unwrap(),
            };
            app_data.date_range = Some(date_range.clone());
            app_data.gtfs = Some(gtfs);
            Ok(date_range)
        }
        Err(e) => {
            println!("Failed to load GTFS data: {}", e);
            Err(e.into())
        }
    }
}

#[tauri::command]
async fn gen_network(model_date: NaiveDate) {
    // TODO: Validate against allowed date.
    println!("Received model date: {}", model_date);
}

#[tauri::command]
fn print_hello() {
    println!("Hello from Rust!");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![load_gtfs, gen_network, print_hello])
        .manage(AppState::default())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
