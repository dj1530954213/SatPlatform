use tauri::Manager; // 重新添加，以备后用或确认是否确实无关

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// 在 Windows 发行版中阻止额外的控制台窗口，请勿删除!!
#[cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
// 进一步了解 Tauri 命令: https://tauri.app/v1/guides/features/command
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

mod config;
mod ws_server;

fn main() {
    // 尽早初始化日志记录器。
    // 建议在 Tauri 构建器设置之前，或者在 setup 闭包的开始处执行此操作。
                         // 根据 P1.1.1 的要求移至 setup 闭包中，以便在需要时访问 app_handle 进行配置。

    tauri::Builder::default()
        .setup(|app| {
            // Initialize logger
            // 初始化日志记录器
            // Ensure this is called only once. 
            // 确保此操作仅被调用一次。
            // Using `try_init` is safer if there's any chance of multiple initializations.
            // 如果有可能发生多次初始化，使用 `try_init` 会更安全。
            if let Err(e) = env_logger::builder().filter_level(log::LevelFilter::Info).try_init() {
                // eprintln! is used here because the logger might not be initialized yet.
                // 此处使用 eprintln! 是因为日志记录器此时可能尚未初始化。
                eprintln!("初始化 env_logger 失败: {}", e);
            }
            log::info!("日志系统已初始化。");

            let handle = app.handle().clone(); // 克隆应用句柄以传递给异步任务
            // 恢复使用 tauri::async_runtime::spawn
            tauri::async_runtime::spawn(async move {
                log::info!("正在创建并启动 WebSocket 服务任务...");
                if let Err(e) = ws_server::service::start_ws_server(handle).await {
                    log::error!("WebSocket 服务启动失败或在运行过程中遇到错误: {:?}", e);
                }
            });
            Ok(())
        })
        // .plugin(tauri_plugin_shell::init()) // Removed this line / 此行已移除 (之前用于 shell 插件，现已解决相关编译错误)
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用时发生错误");
}
