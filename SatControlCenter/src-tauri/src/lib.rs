//! `SatOnSiteMobile` 现场端移动应用的核心库入口点。
//!
//! 本文件通常用于定义库的公共接口（如果适用）和组织内部模块。
//! 对于 Tauri 应用，`main.rs` 中的 `main()` 函数是主要的执行入口，
//! 而 `lib.rs` 的角色可能更多是作为模块的集合点，或者在某些构建配置下（例如移动端）作为入口。

// --- 公开模块声明 --- 
// 这些模块共同构成了现场端应用的核心功能。
pub mod api_client;      // 与外部 API（非 WebSocket）交互的客户端逻辑（如果需要）。
pub mod commands;        // 定义所有可由前端通过 Tauri `invoke` 调用的 Rust 函数。
pub mod config;          // 应用配置加载与管理。
pub mod device_comms;    // 与具体硬件设备通信的逻辑（例如通过串口、蓝牙等）。
pub mod error;           // 定义应用级别的错误类型和处理机制。
pub mod event;           // 定义后端与前端之间通过 Tauri 事件系统传递的事件及其负载结构。
pub mod mobile_specific; // 包含特定于移动平台的功能或适配代码。
pub mod state;           // 定义和管理应用级别的共享状态（除了 Tauri 的托管状态）。
pub mod ws_client;       // WebSocket 客户端服务，负责与云端进行 WebSocket 通信。

/// Tauri 移动端入口点函数。
///
/// 此函数通过 `#[cfg_attr(mobile, tauri::mobile_entry_point)]` 宏标记，
/// 表明在编译为移动端应用时，它将作为应用的起始执行点。
///
/// **注意**: 对于桌面端应用，`main.rs` 中的 `main()` 函数是实际的入口。
/// 此 `run()` 函数的逻辑应与 `main.rs` 中的 `main()` 函数保持一致或进行适当调整，
/// 以确保应用在不同平台上的行为符合预期，特别是关于应用构建和插件初始化的部分。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  // 在标准的 Tauri 应用结构中，日志初始化、Tauri Builder 的配置、
  // 命令注册、setup 钩子以及 .run() 调用主要在 main.rs 中完成。
  // 此处的 run 函数是为了满足 tauri::mobile_entry_point 的要求。
  // 如果 main.rs 中的逻辑需要在此处复用或有特定于移动端的初始化，
  // 则应将相关代码移至此处或共享的辅助函数中。

  // 示例：构建并运行一个基本的 Tauri 应用实例。
  // 实际应用中，这里的配置应与 main.rs 中的 tauri::Builder 配置对齐。
  tauri::Builder::default()
    .setup(|_app| {
      // 日志初始化已在 main.rs 中通过 env_logger 完成。
      // 如果需要为移动端平台配置特定的日志插件（例如 tauri_plugin_log），
      // 可以在此处进行初始化。
      // 例如：
      // if cfg!(debug_assertions) {
      //   _app.handle().plugin(
      //     tauri_plugin_log::Builder::default()
      //       .level(log::LevelFilter::Info) // 设置日志级别
      //       .build(),
      //   )?;
      //   log::info!("[现场端 lib.rs] tauri_plugin_log (移动端日志插件) 已初始化。");
      // }
      log::info!("[现场端 lib.rs] setup 钩子执行。主要的应用初始化逻辑在 main.rs 中。");
      Ok(())
    })
    // .invoke_handler(...) // 如果移动端需要不同的命令集，可以在此配置
    .run(tauri::generate_context!()) // 使用 tauri.conf.json 生成的上下文
    .expect("运行 Tauri 移动应用时发生错误，请检查配置和日志。");
}
