// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// 在 Windows 操作系统的发布 (release) 版本中，此属性用于阻止额外生成一个控制台窗口。
// 重要提示：请勿移除此行代码，除非您明确知道其作用并有充分理由！
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! `SatControlCenter` Tauri 后端主入口。
//!
//! 负责初始化应用、设置 Tauri 构建器、注册命令、管理状态、处理事件，
//! 并最终运行 Tauri 应用。

// --- 本地模块声明 ---
mod commands;
mod config;
mod error;
mod event;
mod ws_client;
// 假设存在 api_client 模块 (如果不需要，后续可以移除)
mod api_client;

// --- 依赖导入 ---
// **移除 use app_lib::...** 

// 使用本地模块
use commands::general_cmds::{ // 导入通用命令
    check_ws_connection_status,
    connect_to_cloud,
    disconnect_from_cloud,
    register_client_with_task,
    send_ws_echo,
};
use config::AppConfig; // 假设配置结构体在本地 config 模块
use ws_client::service::WebSocketClientService; // WebSocket 客户端服务

// Tauri 相关导入
use tauri::{
    // **注释掉系统托盘相关导入**
    // menu::{Menu, MenuItem, PredefinedMenuItem},
    // AppHandle, 
    Manager, 
    RunEvent, 
    // SystemTray, 
    // SystemTrayEvent, 
    Wry, 
    // WindowEvent, 
};
// **移除旧的错误导入**
// use tauri::tray::{SystemTrayEvent, CustomMenuItem, SystemTray, SystemTrayMenu, SystemTrayMenuItem}; // 从 tauri::tray 导入

// 其他标准库或第三方库导入
use std::sync::Arc; // 原子引用计数

// --- 主函数 --- 

fn main() {
    // 初始化日志记录器 (例如使用 env_logger 或 tracing)
    // TODO: 根据项目规则和选择的日志库添加实际的初始化代码
    // env_logger::init(); // 示例：使用 env_logger (旧的)
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init(); // 修改为更灵活的初始化方式
    log::info!("SatControlCenter Tauri 后端启动中...");

    // 创建 WebSocket 客户端服务实例 (需要配置)
    // TODO: 从配置加载 WebSocket URL
    // let app_config = AppConfig::load().unwrap_or_default(); // 示例：加载配置
    // let ws_client_service = Arc::new(WebSocketClientService::new(/* 需要 AppHandle, 可能在 setup 中创建 */)); 

    // 构建 Tauri 应用
    let app_builder = tauri::Builder::default()
        // .manage(ws_client_service) // 状态管理将在 setup 钩子中进行，因为需要 AppHandle
        .setup(|app| {
            log::info!("Tauri setup 钩子执行...");
            // 在 setup 中创建需要 AppHandle 的服务
            let app_handle = app.handle();
            // TODO: 从配置加载 WebSocket URL
            // let app_config = AppConfig::load().unwrap_or_default();
            let ws_client_service = Arc::new(WebSocketClientService::new(app_handle.clone()));
            // 将 WebSocket 服务实例放入 Tauri State
            app.manage(ws_client_service.clone()); 
            log::info!("WebSocketClientService 已创建并放入 Tauri State");

            // TODO: 在这里可以执行其他需要在 setup 阶段完成的初始化工作
            // 例如: 启动后台任务、初始化数据库连接池 (如果需要) 等
            
            Ok(())
        })
        // 注册 Tauri 命令
        .invoke_handler(tauri::generate_handler![
            // 通用命令
            connect_to_cloud,
            disconnect_from_cloud,
            check_ws_connection_status,
            send_ws_echo,
            register_client_with_task,
            // TODO: 添加其他命令模块的命令
        ])
        // **注释掉系统托盘相关调用**
        // .system_tray(create_system_tray())
        // .on_system_tray_event(|app, event| match event { 
        //     SystemTrayEvent::LeftClick { .. } => {
        //         log::debug!("系统托盘左键点击");
        //         let window = app.get_window("main").unwrap(); 
        //         window.show().unwrap();
        //         window.set_focus().unwrap();
        //     }
        //     SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
        //         "quit" => {
        //             log::info!("从系统托盘请求退出应用");
        //             app.exit(0);
        //         }
        //         "show" => {
        //             log::debug!("从系统托盘请求显示窗口");
        //             let window = app.get_window("main").unwrap();
        //             window.show().unwrap();
        //             window.set_focus().unwrap();
        //         }
        //         _ => {}
        //     },
        //     _ => {}
        // })
        // 处理窗口事件 (如果需要)
        .on_window_event(|window, event| match event { 
            tauri::WindowEvent::CloseRequested { api, .. } => {
                log::debug!("主窗口关闭请求事件触发");
                // api.prevent_close(); 
            }
            _ => {}
        });

    // 运行 Tauri 应用并处理运行时事件
    app_builder
        .build(tauri::generate_context!()) 
        .expect("构建 Tauri 应用失败")
        .run(|app_handle, event| match event { 
            RunEvent::ExitRequested { api, .. } => {
                log::info!("Tauri 应用退出请求事件触发");
                // api.prevent_exit(); 
            }
            RunEvent::Ready => {
                log::info!("Tauri 应用准备就绪");
            }
            _ => {}
        });

    log::info!("SatControlCenter Tauri 后端已停止");
}

// --- 辅助函数 (例如创建系统托盘菜单) ---

// **注释掉整个创建系统托盘的函数**
/*
fn create_system_tray() -> SystemTray {
    // 定义菜单项 (使用 tauri::menu::MenuItem)
    let quit = MenuItem::new( "quit".to_string(), "退出");
    let show = MenuItem::new( "show".to_string(), "显示");
    // 创建菜单 (使用 tauri::menu::Menu)
    let tray_menu = Menu::new()
        .add_item(show)
        .add_native_item(PredefinedMenuItem::separator())
        .add_item(quit);

    // 创建系统托盘
    SystemTray::new().with_menu(tray_menu)
}
*/
