//! `SatCloudService` 服务端核心库。
//! 
//! 本 Crate 是 `SatCloudService` Tauri 应用的后端部分，包含了应用的主要业务逻辑、
//! WebSocket 服务、API 接口、配置管理、状态管理等核心功能模块。
//! 
//! 主要模块包括：
//! - `api`: 定义和处理外部 HTTP API 请求。
//! - `config`: 管理应用的配置信息加载与访问。
//! - `db`: (规划中) 数据库交互逻辑。
//! - `error`: 定义应用特定的错误类型。
//! - `mq`: (规划中) 消息队列相关功能。
//! - `state`: 管理应用级别的共享状态。
//! - `ws_server`: 实现 WebSocket 服务端，处理客户端连接、消息路由和实时通信。

pub mod api;
pub mod config;
pub mod db;
pub mod error;
pub mod mq;
pub mod state;
pub mod ws_server;

/// Tauri 应用的移动端入口点 (当 `mobile` feature 被激活时)。
/// 对于桌面应用，此属性通常不起作用，但包含它有助于跨平台兼容性。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Tauri 应用的主运行函数。
/// 
/// 此函数配置并启动 Tauri 应用实例。主要步骤包括：
/// 1. 创建一个默认的 `tauri::Builder`。
/// 2. 在 `setup` 钩子中进行应用初始化设置：
///    - 对于调试构建 (`debug_assertions` 为 true)，初始化并注册 `tauri_plugin_log` 插件，
///      以便将 `log` crate 的日志输出到控制台或 Tauri 的日志系统。
///      默认日志级别设置为 `Info`。
/// 3. 调用 `run` 方法，传入通过 `tauri::generate_context!()` 宏生成的应用上下文，
///    这会启动 Tauri 的事件循环和 WebView。
/// 4. 如果 `run` 方法返回错误（例如，无法启动应用），则调用 `expect` 抛出一个 panic，
///    指示应用启动失败。
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // 仅在调试模式下启用日志插件，以方便开发和调试。
            // 在生产环境中，可以考虑移除此插件或使用更高级的日志配置。
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info) // 设置默认日志记录级别为 Info
                        .build(),
                )?;
                log::info!("[主程序] Tauri 日志插件已在调试模式下初始化。");
            }
            Ok(())
        })
        .run(tauri::generate_context!()) // 运行 Tauri 应用，传入自动生成的上下文
        .expect("[主程序] 致命错误：运行 Tauri 应用时发生严重问题，应用无法启动。"); // 如果运行失败，则 panic
}
