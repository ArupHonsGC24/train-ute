#[tauri::command]
fn button_cmd_handler(cmd: &str) {
    println!("{cmd} was pressed!");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![button_cmd_handler])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
