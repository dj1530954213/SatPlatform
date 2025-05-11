//! `SatControlCenter` (卫星控制中心) 应用的核心库入口模块。
//!
//! 本模块 (`lib.rs`) 作为 `SatControlCenter` 这个 crate (包) 的根模块，主要承担以下职责：
//! - **模块组织与声明**: 声明并组织构成本应用核心逻辑的所有子模块。
//!   每个子模块都封装了特定的功能领域，例如：
//!   - `api_client`: (预期功能) 与外部 HTTP API 服务进行交互的客户端逻辑。
//!   - `commands`: 定义所有可供前端调用的 Tauri 后端命令。
//!   - `config`: 应用配置的加载、管理和持久化。
//!   - `error`: 定义应用专属的错误类型。
//!   - `event`: 定义用于后端与前端通信的 Tauri 事件。
//!   - `plc_comms`: (预期功能) 与 PLC (可编程逻辑控制器) 设备进行通信的逻辑。
//!   - `state`: (预期功能) 管理应用级别的共享状态。
//!   - `ws_client`: 封装与云端服务进行 WebSocket 通信的客户端逻辑。
//! - **应用启动逻辑 (特定场景)**: 包含一个 `run()` 公开函数。此函数主要用于在特定构建配置
//!   (例如，当 `SatControlCenter` 被编译为移动端应用，由 `#[cfg_attr(mobile, tauri::mobile_entry_point)]` 属性指明时)
//!   作为应用的入口点来启动 Tauri 应用实例。在标准的桌面应用场景下，`main.rs` 中的 `main()` 函数通常是主入口。
//!   在 `run()` 函数内部，会进行 Tauri 应用的构建，包括插件的初始化 (如 `tauri_plugin_log`)。

pub mod api_client;
pub mod commands;
pub mod config;
pub mod error;
pub mod event;
pub mod plc_comms;
pub mod state;
pub mod ws_client;

/// Tauri 移动端入口点属性。
/// 此 `#[cfg_attr(mobile, tauri::mobile_entry_point)]` 属性是一个条件编译属性。
/// 它表示：仅当编译目标为 `mobile` (移动端平台，例如通过 `tauri android build` 或 `tauri ios build` 命令构建时)
/// 被激活时，下面的 `run` 函数才会被标记为 Tauri 的移动端应用入口点。
/// 对于非移动端构建 (例如标准的桌面应用构建)，此属性无效，`main.rs` 中的 `main()` 函数将作为主入口。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// `SatControlCenter` (卫星控制中心) 应用的备用/特定场景启动函数。
///
/// 此函数主要设计用于以下情况：
/// - 当 `SatControlCenter` 被编译为针对移动端平台 (Android, iOS) 的应用时，
///   它将通过 `#[cfg_attr(mobile, tauri::mobile_entry_point)]` 属性被指定为应用的入口点。
/// - 如果 `SatControlCenter` 未来被设计为一个可以作为 Tauri 插件被其他 Tauri 应用集成时，
///   此 `run` 函数也可能作为插件初始化或启动其核心服务的入口。
///
/// 函数内部的逻辑包括：
/// 1.  使用 `tauri::Builder::default()` 创建一个 Tauri 应用构建器。
/// 2.  在 `setup` 钩子中，有条件地 (仅在 `debug_assertions` (调试断言) 开启时，即通常在开发和调试构建中)
///     初始化并注册 `tauri_plugin_log` 插件。此插件提供了将 Rust 后端的 `log` crate 产生的日志
///     桥接到前端开发者控制台或特定日志文件的能力，便于调试。
///     日志级别被设置为 `log::LevelFilter::Info`，意味着 Info 及以上级别 (Info, Warn, Error) 的日志将被处理。
/// 3.  调用 `.run(tauri::generate_context!())` 来最终构建并运行 Tauri 应用实例。
/// 4.  如果应用运行过程中发生无法恢复的错误导致启动失败，则通过 `.expect(...)` 抛出一个 panic (程序恐慌)。
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
    .expect("启动 SatControlCenter Tauri 应用时发生严重错误，无法继续运行。");
}
