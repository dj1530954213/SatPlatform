//! `SatControlCenter` 控制中心核心逻辑。

pub mod api_client;
pub mod commands;
pub mod config;
pub mod error;
pub mod event;
pub mod plc_comms;
pub mod state;
pub mod ws_client;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
