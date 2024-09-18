use chrono::NaiveDate;
use gtfs_structures::{Gtfs, GtfsReader};
use raptor::Network;
use std::fs::File;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{ipc, AppHandle, Emitter, State};
use tauri_plugin_dialog::{DialogExt, FilePath};
use train_ute::{data_export, data_import, simulation};

#[derive(Debug, thiserror::Error)]
enum CmdError {
    #[error("Unexpected request body.")]
    RequestBodyMustBeRaw,
    /*    #[error("Missing header entry: `{0}`.")]
        MissingHeader(&'static str),
        #[error("Malformed header entry: `{0}`.")]
        MalformedHeaderEntry(&'static str),*/
    #[error("Prerequisite unsatisfied: `{0}`.")]
    PrerequisiteUnsatisfied(&'static str),
    #[error("Path conversion error: {0}.")]
    PathConversion(FilePath),
    #[error("Invalid filename in filepath: {0}.")]
    InvalidFilename(PathBuf),
    #[error("Mutex poisoned.")]
    MutexPoisoned,
    #[error("IO error: {0}.")]
    Io(#[from] std::io::Error),
    #[error("GTFS error: {0}.")]
    Gtfs(#[from] gtfs_structures::Error),
    #[error("Tauri error: {0}.")]
    Tauri(#[from] tauri::Error),
    #[error("Data import error: {0}.")]
    DataImport(#[from] data_import::DataImportError),
    #[error("Data export error: {0}.")]
    DataExport(#[from] data_export::DataExportError),
}

type CmdResult<T> = Result<T, CmdError>;

// Can't contain a poison error in returned error because it allows access to the mutex.
impl<T> From<std::sync::PoisonError<T>> for CmdError {
    fn from(_err: std::sync::PoisonError<T>) -> Self {
        Self::MutexPoisoned
    }
}

impl serde::Serialize for CmdError {
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
    sim_steps: Option<Vec<simulation::SimulationStep>>,
    sim_result: Option<simulation::SimulationResult>,
    path_data: Vec<u8>,
    trip_data: Vec<u8>,
}

impl AppStateData {
    pub fn get_loaded_gtfs(&self) -> CmdResult<&LoadedGtfs> {
        self.loaded_gtfs.as_ref().ok_or(CmdError::PrerequisiteUnsatisfied("GTFS data must be loaded first."))
    }
    pub fn get_network(&self) -> CmdResult<&Network> {
        self.network.as_ref().ok_or(CmdError::PrerequisiteUnsatisfied("Network must be generated first."))
    }
    pub fn get_sim_steps(&self) -> CmdResult<&Vec<simulation::SimulationStep>> {
        self.sim_steps.as_ref().ok_or(CmdError::PrerequisiteUnsatisfied("Patronage data must be imported first."))
    }
    pub fn get_sim_result(&self) -> CmdResult<&simulation::SimulationResult> {
        self.sim_result.as_ref().ok_or(CmdError::PrerequisiteUnsatisfied("Simulation must be run first."))
    }
}

#[derive(Default)]
struct AppState {
    data: Mutex<AppStateData>,
}

#[tauri::command]
async fn load_gtfs(request: ipc::Request<'_>, state: State<'_, AppState>) -> CmdResult<DateRange> {
    let ipc::InvokeBody::Raw(gtfs_zip) = request.body() else {
        return Err(CmdError::RequestBodyMustBeRaw);
    };

    // Load GTFS data. TODO: Why does this take so long? Probably because it was a debug build.
    // TODO: Refactor this into a separate function (and put in a separate frontend crate?).
    match GtfsReader::default().raw().read_from_reader(Cursor::new(gtfs_zip)).and_then(Gtfs::try_from) {
        Ok(gtfs) => {
            if gtfs.shapes.is_empty() {
                return Err(CmdError::PrerequisiteUnsatisfied("GTFS data must contain shapes."));
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
async fn gen_network(model_date: NaiveDate, state: State<'_, AppState>) -> CmdResult<()> {
    let mut app_data = state.data.lock()?;

    let loaded_gtfs = app_data.get_loaded_gtfs()?;

    if model_date < loaded_gtfs.date_range.min || model_date > loaded_gtfs.date_range.max {
        return Err(CmdError::PrerequisiteUnsatisfied("Model date must be within the GTFS date range."));
    }

    // TODO: Make user specifiable.
    let default_transfer_time = 3 * 60;

    let mut network = Network::new(&loaded_gtfs.gtfs, model_date, default_transfer_time);
    network.build_connections();

    // Line shapes are constant for the network, so calculate here.
    app_data.path_data = Vec::new();
    // TODO rename data export functions (as they are now used in-process).
    data_export::export_shape_file(&network, &mut app_data.path_data)?;

    app_data.network = Some(network);

    Ok(())
}

const PARQUET_FILTER: &[&str] = &["parquet", "pq"];

#[tauri::command]
async fn patronage_data_import(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let mut app_data = state.data.lock()?;
    let network = app_data.get_network()?;

    // Dummy data:
    // let num_agents = 72000;
    // app_data.sim_steps = Some(simulation::gen_simulation_steps(&network, Some(num_agents), Some(0)));

    let Some(filepath) = app.dialog()
                            .file()
                            .add_filter("Parquet", PARQUET_FILTER)
                            .blocking_pick_file() else {
        // User cancelled.
        return Ok(());
    };
    let filepath = filepath.as_path().ok_or(CmdError::PathConversion(filepath.clone()))?;

    let datafile = File::open(filepath)?;

    app_data.sim_steps = Some(data_import::import_patronage_date(datafile, network)?);

    Ok(())
}

#[tauri::command]
async fn run_simulation(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let mut app_data = state.data.lock()?;

    let network = app_data.get_network()?;
    let simulation_steps = app_data.get_sim_steps()?;

    let params = simulation::DefaultSimulationParams::new(794, Some(|progress| {
        app.emit("simulation-progress", progress).unwrap();
    }));

    let sim_result = Some(simulation::run_simulation::<_, true>(network, &simulation_steps, &params));

    // Export the trip data
    let mut trip_data = Vec::new();
    data_export::export_network_trips(&network, &sim_result.as_ref().unwrap(), &mut trip_data)?;

    app_data.sim_result = sim_result;
    app_data.trip_data = trip_data;

    Ok(())
}

#[tauri::command]
async fn export_results(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let app_data = state.data.lock()?;

    let network = app_data.get_network()?;
    let sim_result = app_data.get_sim_result()?;

    // TODO: Separate dialogs for each export type?
    let Some(filepath) = app.dialog()
                            .file()
                            .set_file_name("result")
                            .add_filter("Parquet", PARQUET_FILTER)
                            .blocking_save_file() else {
        // User cancelled.
        return Ok(());
    };

    // TODO: format with rustfmt.
    let filepath = filepath.as_path().ok_or(CmdError::PathConversion(filepath.clone()))?;
    let filename = filepath.file_stem()
                           .ok_or(CmdError::InvalidFilename(filepath.to_owned()))?
        .to_str()
        .ok_or(CmdError::InvalidFilename(filepath.to_owned()))?;

    println!("{filename}");
    println!("{}", filepath.with_file_name(format!("{filename}_counts")).display());

    println!("Exporting agent counts to: {}", filepath.with_file_name(format!("{filename}_counts")).display());
    data_export::export_agent_counts(&filepath.with_file_name(format!("{filename}_counts")), network, sim_result)?;

    println!("Exporting agent journeys to: {}", filepath.with_file_name(format!("{filename}_journeys")).display());
    data_export::export_agent_journeys(
        File::create(
            filepath.with_file_name(&format!("{filename}_journeys")).with_extension("parquet")
        )?,
        network,
        sim_result)?;

    Ok(())
}

#[tauri::command]
fn get_path_data(state: State<'_, AppState>) -> CmdResult<ipc::Response> {
    let app_data = state.data.lock()?;
    Ok(ipc::Response::new(app_data.path_data.clone()))
}

#[tauri::command]
fn get_trip_data(state: State<'_, AppState>) -> CmdResult<ipc::Response> {
    let app_data = state.data.lock()?;
    Ok(ipc::Response::new(app_data.trip_data.clone()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            load_gtfs, 
            gen_network, 
            run_simulation, 
            patronage_data_import, 
            export_results, 
            get_trip_data, 
            get_path_data
        ])
        .manage(AppState::default())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
