use std::io::Cursor;
use std::sync::Mutex;

use chrono::NaiveDate;
use gtfs_structures::{Gtfs, GtfsReader};
use raptor::Network;
use tauri::{ipc, AppHandle, Emitter, State};
use train_ute::{data_export, simulation};

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Unexpected request body.")]
    RequestBodyMustBeRaw,
    /*    #[error("Missing header entry: `{0}`.")]
        MissingHeader(&'static str),
        #[error("Malformed header entry: `{0}`.")]
        MalformedHeaderEntry(&'static str),*/
    #[error("Prerequisite unsatisfied: `{0}`.")]
    PrerequisiteUnsatisfied(&'static str),
    #[error("Data export error: {0}.")]
    DataExport(#[from] data_export::DataExportError),
    #[error("Mutex poisoned.")]
    MutexPoisoned,
    #[error("IO error: {0}.")]
    Io(#[from] std::io::Error),
    #[error("GTFS error: {0}.")]
    Gtfs(#[from] gtfs_structures::Error),
    #[error("Tauri error: {0}.")]
    Tauri(#[from] tauri::Error),
}

// Can't contain a poison error in returned error because it allows access to the mutex.
impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_err: std::sync::PoisonError<T>) -> Self {
        Self::MutexPoisoned
    }
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

struct LoadedGtfs {
    gtfs: Gtfs,
    date_range: DateRange,
}

#[derive(Default)]
struct AppStateData {
    loaded_gtfs: Option<LoadedGtfs>,
    network: Option<Network>,
    sim_result: Option<simulation::SimulationResult>,
    path_data: Vec<u8>,
    trip_data: Vec<u8>,
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
            if gtfs.shapes.is_empty() {
                return Err(Error::PrerequisiteUnsatisfied("GTFS data must contain shapes."));
            }

            println!("Successfully loaded GTFS data in {}ms.", gtfs.read_duration);
            let mut app_data = state.data.lock()?;
            let date_range = DateRange {
                // TODO: Handle empty calendar and look at calendar_dates too.
                min: gtfs.calendar.values().map(|c| c.start_date).min().unwrap(),
                max: gtfs.calendar.values().map(|c| c.end_date).max().unwrap(),
            };
            app_data.loaded_gtfs = Some(LoadedGtfs { gtfs, date_range: date_range.clone() });
            Ok(date_range)
        }
        Err(e) => {
            println!("Failed to load GTFS data: {}", e);
            Err(e.into())
        }
    }
}

#[tauri::command]
async fn gen_network(model_date: NaiveDate, state: State<'_, AppState>) -> Result<(), Error> {
    let mut app_data = state.data.lock()?;

    let Some(loaded_gtfs) = app_data.loaded_gtfs.as_ref() else {
        return Err(Error::PrerequisiteUnsatisfied("GTFS data must be loaded first."));
    };

    if model_date < loaded_gtfs.date_range.min || model_date > loaded_gtfs.date_range.max {
        return Err(Error::PrerequisiteUnsatisfied("Model date must be within the GTFS date range."));
    }

    // TODO: Make user specifiable.
    let default_transfer_time = 3 * 60;

    let mut network = Network::new(&loaded_gtfs.gtfs, model_date, default_transfer_time);
    network.build_connections();

    // Line shapes are constant for the network, so calculate here.
    app_data.path_data = Vec::new();
    data_export::export_shape_file(&network, &mut app_data.path_data)?;

    app_data.network = Some(network);

    Ok(())
}

#[tauri::command]
async fn run_simulation(app: AppHandle, state: State<'_, AppState>) -> Result<(), Error> {
    let mut app_data = state.data.lock()?;

    let Some(network) = app_data.network.as_ref() else {
        return Err(Error::PrerequisiteUnsatisfied("Network must be generated first."));
    };

    // TODO: use data import.
    let num_agents = 72000;
    let simulation_steps = simulation::gen_simulation_steps(&network, Some(num_agents), Some(0));

    let params = simulation::DefaultSimulationParams::new_with_callback(794, |progress| {
        app.emit("simulation-progress", progress).unwrap();
    });

    let sim_result = Some(simulation::run_simulation::<_, true>(network, &simulation_steps, &params));

    // Export the trip data
    let mut trip_data = Vec::new();
    data_export::export_network_trips(&network, &sim_result.as_ref().unwrap(), &mut trip_data)?;

    app_data.sim_result = sim_result;
    app_data.trip_data = trip_data;

    Ok(())
}

#[tauri::command]
fn export_results() {
    println!("export results.");
}

#[tauri::command]
async fn get_path_data(state: State<'_, AppState>) -> Result<ipc::Response, Error> {
    let app_data = state.data.lock()?;
    Ok(ipc::Response::new(app_data.path_data.clone()))
}

#[tauri::command]
async fn get_trip_data(state: State<'_, AppState>) -> Result<ipc::Response, Error> {
    let app_data = state.data.lock()?;
    Ok(ipc::Response::new(app_data.trip_data.clone()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![load_gtfs, gen_network, run_simulation, export_results, get_trip_data, get_path_data])
        .manage(AppState::default())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
