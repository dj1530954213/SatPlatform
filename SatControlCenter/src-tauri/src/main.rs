// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// 在 Windows 操作系统的发布 (release) 版本中，此属性用于阻止额外生成一个控制台窗口。
// 重要提示：请勿移除此行代码，除非您明确知道其作用并有充分理由！
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager; // Tauri 核心库，用于应用管理和状态管理等功能。

// --- 模块声明与导入 --- 
// `config` 模块：负责应用配置的加载、管理和持久化。
mod config;
// `event` 模块：(预期功能) 处理应用级别的事件，例如来自Tauri窗口的事件或自定义事件。
mod event;
// `ws_client` 模块：核心模块，封装了与云端服务 (`SatCloudService`) 进行 WebSocket 通信的客户端逻辑。
mod ws_client;
// `commands` 模块：定义了所有可供前端 Angular 应用通过 `invoke` 调用的 Tauri 后端命令。
mod commands;

use std::sync::Arc; // `Arc` (原子引用计数)，用于在多处安全地共享 `WebSocketClientService` 实例的所有权。
use ws_client::WebSocketClientService; // 从 `ws_client` 模块导入核心的 WebSocket 客户端服务结构体。

/// `SatControlCenter` (卫星控制中心) 应用的主入口函数。
fn main() {
    // --- 日志系统初始化 --- 
    // 根据项目规则 (P2.1.1)，明确要求使用 `env_logger` 或 `tracing-subscriber`作为日志后端。
    // 此处选择并初始化 `env_logger`。
    // 检查环境变量 `RUST_LOG` 是否已设置。如果未设置，则将其默认设置为 "debug" 级别，
    // 这意味着在没有外部通过环境变量指定日志级别时，应用将默认记录 debug 及以上级别 (debug, info, warn, error) 的日志。
    // 注意：在生产环境中，通常建议将默认级别设置为 "info" 或 "warn"，并通过环境变量按需调整为 "debug" 或 "trace"。
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "debug"); // 设置默认日志输出级别为 debug
    }
    env_logger::init(); // 初始化 env_logger 日志记录器
    log::info!("SatControlCenter (卫星控制中心) 应用开始启动...");

    // --- Tauri 应用构建与运行 --- 
    tauri::Builder::default() // 使用 Tauri 提供的默认构建器开始构建应用
        .setup(|app| { // `setup` 钩子：在 Tauri 应用核心初始化完成、但窗口尚未创建之前执行一次性设置逻辑。
            log::info!("Tauri setup 钩子执行：开始初始化 WebSocketClientService (WebSocket客户端服务)... ");
            // 1. 创建 WebSocketClientService (WebSocket客户端服务) 实例。
            //    `app.handle()` 返回一个 `AppHandle` (应用句柄)，它可以被安全地克隆并在不同线程间传递，
            //    `WebSocketClientService` 可能需要它来与 Tauri API (例如发送事件) 进行交互。
            //    使用 `Arc` 将服务实例包裹起来，以便后续可以安全地共享它 (例如，通过 Tauri 的托管状态)。
            let ws_service = Arc::new(WebSocketClientService::new(app.handle().clone()));
            
            // 2. 将创建的 `ws_service` (WebSocket客户端服务) 实例放入 Tauri 的托管状态 (Managed State) 中。
            //    通过 `app.manage(ws_service)`，该服务实例可以在后续的 Tauri 命令 (`#[tauri::command]`) 中
            //    通过类型注入 (例如，`state: tauri::State<Arc<WebSocketClientService>>`) 的方式被安全地访问和使用。
            app.manage(ws_service);
            log::info!("WebSocketClientService (WebSocket客户端服务) 实例已成功创建并被放入 Tauri 托管状态 (Managed State)。");
            Ok(()) // `setup` 钩子需要返回 `Result`，`Ok(())` 表示初始化成功。
        })
        .invoke_handler(tauri::generate_handler![
            // --- Tauri 命令注册 --- 
            // 此处列出了所有通过 `tauri::generate_handler!` 宏注册到应用的 Tauri 后端命令。
            // 这些命令定义在 `commands` 模块及其子模块中，可供前端 Angular 应用通过 `invoke` API 调用。
            // 例如：`commands::general_cmds::connect_to_cloud` 是一个用于连接到云端 WebSocket 服务的命令。
            commands::general_cmds::connect_to_cloud,       // 连接到云端服务命令
            commands::general_cmds::disconnect_from_cloud,  // 从云端服务断开连接命令
            commands::general_cmds::check_ws_connection_status, // 检查 WebSocket 连接状态命令
            commands::general_cmds::send_ws_echo            // 发送 WebSocket Echo (回声) 测试消息命令
            // 提示：未来若添加新的 Tauri 命令，请记得在此处注册它们。
        ])
        .run(tauri::generate_context!()) // 运行 Tauri 应用，`generate_context!` 会在编译时加载 `tauri.conf.json` 和其他上下文信息。
        .expect("运行 SatControlCenter (卫星控制中心) Tauri 应用时发生严重错误，无法启动。"); // 如果 `run` 方法返回错误 (例如，无法初始化 Webview)，则程序会 panic 并显示此消息。
}
