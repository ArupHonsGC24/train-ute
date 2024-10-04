use std::collections::HashMap;
use std::fs::File;
use std::io::Cursor;
use std::sync::Mutex;

use chrono::NaiveDate;
use gtfs_structures::{Gtfs, GtfsReader};
use raptor::journey::JourneyPreferences;
use raptor::Network;
use tauri::ipc::Channel;
use tauri::{ipc, AppHandle, State};
use tauri_plugin_dialog::{DialogExt, FilePath};
use raptor::network::PathfindingCost;
use train_ute::{data_export, data_import, simulation};
use train_ute::simulation::TripCapacity;
use train_ute::simulation::CrowdingCost;

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
    trip_capacities: HashMap<String, TripCapacity>,
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

    // Load GTFS data.
    // TODO: Refactor this into a separate function (and put in a separate frontend crate?).
    match GtfsReader::default().raw().read_from_reader(Cursor::new(gtfs_zip)).and_then(Gtfs::try_from) {
        Ok(gtfs) => {
            if gtfs.shapes.is_empty() {
                return Err(CmdError::PrerequisiteUnsatisfied("GTFS data must contain shapes."));
            }

            log::info!("Successfully loaded GTFS data in {}ms.", gtfs.read_duration);
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
            log::warn!("Failed to load GTFS data: {}", e);
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

    app_data.sim_steps = Some(data_import::build_simulation_steps_from_patronage_data(datafile, network)?);

    Ok(())
}

#[tauri::command]
async fn import_trip_capacities(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let mut app_data = state.data.lock()?;

    let Some(filepath) = app.dialog()
                            .file()
                            .add_filter("CSV", &["csv"])
                            .blocking_pick_file() else {
        // User cancelled.
        return Ok(());
    };
    let filepath = filepath.as_path().ok_or(CmdError::PathConversion(filepath.clone()))?;

    let datafile = File::open(filepath)?;

    app_data.trip_capacities = data_import::import_trip_capacities(datafile)?;

    Ok(())
}

#[tauri::command]
async fn export_model_csv(crowding_model: simulation::CrowdingModel, app: AppHandle) -> CmdResult<()> {
    let Some(filepath) = app.dialog()
                            .file()
                            .set_file_name(crowding_model.func.get_name().to_owned() + "_model")
                            .add_filter("CSV", &["csv"])
                            .blocking_save_file() else {
        // User cancelled.
        return Ok(());
    };

    let csv = crowding_model.generate_csv();

    let filepath = filepath.as_path().ok_or(CmdError::PathConversion(filepath.clone()))?;
    std::fs::write(filepath, csv)?;

    Ok(())
}

// Serializable versions of crowding model types.
//#[derive(serde::Deserialize, Debug)]
//#[serde(remote = "simulation::CrowdingFunc")]
//enum CrowdingFuncDef {
//    Linear,
//    Quadratic,
//    OneStep,
//    TwoStep,
//}
//
//#[derive(serde::Deserialize, Debug)]
//#[serde(remote = "simulation::CrowdingModel")]
//struct CrowdingModelDef {
//    #[serde(with = "CrowdingFuncDef")]
//    func: simulation::CrowdingFunc,
//    seated: u32,
//    standing: u32,
//}
//
//#[derive(serde::Deserialize)]
//struct CrowdingModelDeserializeHelper(#[serde(with = "CrowdingModelDef")] simulation::CrowdingModel);

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
enum SimulationEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        num_rounds: u16,
        num_steps: usize,
    },
    StepCompleted,
    //RoundCompleted,
}

#[tauri::command]
async fn run_simulation(num_rounds: u16,
                        bag_size: usize,
                        cost_utility: CrowdingCost,
                        crowding_model: simulation::CrowdingModel,
                        should_report_progress: bool,
                        on_simulation_event: Channel<SimulationEvent>,
                        state: State<'_, AppState>) -> CmdResult<()> {
    let mut app_data = state.data.lock()?;

    let network = app_data.get_network()?;
    let simulation_steps = app_data.get_sim_steps()?;

    on_simulation_event.send(SimulationEvent::Started { num_rounds, num_steps: simulation_steps.len() }).unwrap_or_else(|e| {
        log::warn!("Error sending init event: {e}");
    });

    let journey_preferences = JourneyPreferences {
        utility_function: Box::new(move |label, start_time| {
            (label.arrival_time - start_time) as PathfindingCost + cost_utility * label.cost
        })
    };

    let params = simulation::DefaultSimulationParams {
        crowding_model,
        progress_callback: Some(Box::new(|| {
            on_simulation_event.send(SimulationEvent::StepCompleted).unwrap_or_else(|e| {
                log::warn!("Error sending progress event: {e}");
            });
        })),
        journey_preferences,
        num_rounds,
        bag_size,
        trip_capacities: app_data.trip_capacities.clone(),
        should_report_progress,
    };

    let sim_result = Some(simulation::run_simulation(network, &simulation_steps, &params));

    // Export the trip data
    let mut trip_data = Vec::new();
    data_export::export_network_trips(&network, &sim_result.as_ref().unwrap(), &mut trip_data)?;

    app_data.sim_result = sim_result;
    app_data.trip_data = trip_data;

    Ok(())
}

#[tauri::command]
async fn export_counts(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let app_data = state.data.lock()?;

    let network = app_data.get_network()?;
    let sim_result = app_data.get_sim_result()?;

    let Some(filepath) = app.dialog()
                            .file()
                            .set_file_name("agent_counts")
                            .add_filter("Parquet", PARQUET_FILTER)
                            .blocking_save_file() else {
        // User cancelled.
        return Ok(());
    };

    let filepath = filepath.as_path().ok_or(CmdError::PathConversion(filepath.clone()))?;
    data_export::export_agent_counts(filepath, network, sim_result)?;

    Ok(())
}
#[tauri::command]
async fn export_journeys(legs: bool, app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let app_data = state.data.lock()?;

    let network = app_data.get_network()?;
    let sim_result = app_data.get_sim_result()?;

    let filename = if legs {
        "legs"
    } else {
        "journeys"
    };

    let Some(filepath) = app.dialog()
                            .file()
                            .set_file_name(filename)
                            .add_filter("Parquet", PARQUET_FILTER)
                            .blocking_save_file() else {
        // User cancelled.
        return Ok(());
    };

    let filepath = filepath.as_path().ok_or(CmdError::PathConversion(filepath.clone()))?;
    data_export::export_agent_journeys(File::create(filepath.with_extension("parquet"))?, network, sim_result, legs)?;

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
    // Set up logging.
    let log_env = env_logger::Env::default()
        .filter_or("TRAIN_UTE_LOG_LEVEL", "info")
        .write_style_or("TRAIN_UTE_LOG_STYLE", "always");
    env_logger::init_from_env(log_env);

    // Run tauri.
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            load_gtfs, 
            gen_network, 
            patronage_data_import,
            import_trip_capacities,
            export_model_csv,
            run_simulation, 
            export_counts, 
            export_journeys, 
            get_trip_data, 
            get_path_data
        ])
        .manage(AppState::default())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
