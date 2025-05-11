//! `SatOnSiteMobile` 现场端移动应用核心逻辑。

pub mod api_client;
pub mod commands;
pub mod config;
pub mod device_comms;
pub mod error;
pub mod event;
pub mod mobile_specific;
pub mod state;
pub mod ws_client;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .setup(|_app| {
      // 日志初始化已在 main.rs 中通过 env_logger 完成
      // 因此移除此处的 tauri_plugin_log 初始化
      // if cfg!(debug_assertions) {
      //   _app.handle().plugin(
      //     tauri_plugin_log::Builder::default()
      //       .level(log::LevelFilter::Info)
      //       .build(),
      //   )?;
      // }
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
