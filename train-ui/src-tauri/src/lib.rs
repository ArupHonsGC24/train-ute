use std::sync::Mutex;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_fs::FilePath;

use chrono::NaiveDate;
use gtfs_structures::{Gtfs, GtfsReader};
use tauri::{AppHandle, State};

#[derive(Default)]
struct AppStateData {
    gtfs: Option<Gtfs>,
    date_range: Option<(NaiveDate, NaiveDate)>,
}

#[derive(Default)]
struct AppState {
    pub data: Mutex<AppStateData>,
}

#[tauri::command]
async fn load_gtfs<'a>(app: AppHandle, state: State<'a, AppState>) -> Result<(), ()> {
    if let Some(FilePath::Path(gtfs_path)) = app.dialog().file().blocking_pick_file() {
        match GtfsReader::default().read_from_path(gtfs_path.to_string_lossy().to_string()) {
            Ok(gtfs) => {
                println!("Successfully loaded GTFS data.");
                let mut app_data = state.data.lock().unwrap();
                app_data.date_range = Some((NaiveDate::from_ymd(2024, 1, 1), NaiveDate::from_ymd(2024, 1, 7)));
                app_data.gtfs = Some(gtfs);
            }
            Err(e) => {
                println!("Failed to load GTFS data: {}", e);
            }
        }
    } else {
        println!("No file selected.");
    }
    Ok(())
}

#[tauri::command]
fn gen_network(gtfs_zip: Vec<u8>) {
    println!("Received GTFS zip: {}", gtfs_zip.len());
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
