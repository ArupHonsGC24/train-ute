use std::io::Cursor;
use std::sync::Mutex;

use chrono::NaiveDate;
use gtfs_structures::{Gtfs, GtfsReader};
use tauri::{ipc, State};

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Unexpected request body.")]
    RequestBodyMustBeRaw,
    #[error("Missing header entry: `{0}`.")]
    MissingHeader(&'static str),
    #[error("Malformed header entry: `{0}`.")]
    MalformedHeaderEntry(&'static str),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("GTFS error: {0}")]
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

#[derive(Default)]
struct AppStateData {
    gtfs: Option<Gtfs>,
    date_range: Option<(NaiveDate, NaiveDate)>,
}

#[derive(Default)]
struct AppState {
    data: Mutex<AppStateData>,
}

#[tauri::command]
async fn gen_network(request: ipc::Request<'_>, state: State<'_, AppState>) -> Result<(), Error> {
    let ipc::InvokeBody::Raw(gtfs_zip) = request.body() else {
        return Err(Error::RequestBodyMustBeRaw);
    };
    println!("Received GTFS zip: {}", gtfs_zip.len());

    // Parse model date from header entry. TODO: Validate.
    const MODEL_DATE_ID: &str = "model_date";
    let Some(model_date) = request.headers().get(MODEL_DATE_ID) else {
      return Err(Error::MissingHeader(MODEL_DATE_ID));
    };
    let model_date = model_date.to_str().map_err(|_| Error::MalformedHeaderEntry(MODEL_DATE_ID))?;
    let model_date = NaiveDate::parse_from_str(model_date, "%Y-%m-%d").map_err(|_| Error::MalformedHeaderEntry(MODEL_DATE_ID))?;

    println!("Received model date: {}", model_date);

    // Load GTFS data. TODO: Why does this take so long?
    match GtfsReader::default().raw().read_from_reader(Cursor::new(gtfs_zip)).and_then(Gtfs::try_from) {
        Ok(gtfs) => {
            println!("Successfully loaded GTFS data in {}ms.", gtfs.read_duration);
            let mut app_data = state.data.lock().unwrap();
            app_data.date_range = None; //Some((NaiveDate::from_ymd(2024, 1, 1), NaiveDate::from_ymd(2024, 1, 7)));
            app_data.gtfs = Some(gtfs);
        }
        Err(e) => {
            println!("Failed to load GTFS data: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![gen_network])
        .manage(AppState::default())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
