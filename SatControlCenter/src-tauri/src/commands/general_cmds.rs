// SatControlCenter/src-tauri/src/commands/general_cmds.rs

//! `SatControlCenter` (卫星控制中心) 应用中与通用控制和连接管理相关的 Tauri 命令模块。
//!
//! 本模块 (`general_cmds.rs`) 包含了一系列不特定于某个具体业务领域 (如任务管理或特定数据操作)，
//! 而是服务于整个应用或核心功能的通用 Tauri 后端命令。这些命令通常涉及：
//! - **网络连接管理**: 例如，与云端 `SatCloudService` 的 WebSocket 连接的建立、断开和状态检查。
//! - **基础通信测试**: 例如，发送 Echo (回声) 消息以验证通信链路的完整性。
//! - **应用级操作**: (未来可能) 例如，获取应用版本信息、触发应用级别的诊断等。
//!
//! 这些命令是前端 Angular 应用与后端 Rust 逻辑进行交互的主要接口之一，
//! 通过 Tauri 的 `invoke` API 被调用。

use tauri::State; // Tauri 核心库，用于在命令中注入托管状态 (Managed State)。
use log::{info, error, debug, warn}; // `log` crate，用于在后端记录不同级别的日志信息 (信息、错误等)。
// use crate::ws_client::service::WsClientService; // 旧的或错误的导入路径，已被注释或将在后续清理中移除。
use crate::ws_client::WebSocketClientService; // 从 `ws_client` 模块导入核心的 WebSocket 客户端服务结构体。
use common_models::{ // 直接从 common_models 根导入
    ClientRole,
    ws_payloads::{EchoPayload, RegisterPayload},
};
use chrono::Utc; // `chrono` crate，用于获取当前的 UTC 时间戳。
use uuid::Uuid; // `uuid` crate，用于生成唯一的 UUID (通用唯一标识符)，例如为 `WsMessage` 生成 `message_id`。
use tauri::AppHandle; // Tauri 应用句柄，提供对应用核心功能的访问，例如发送事件、管理窗口等。在此处主要用于Tauri的State注入机制，服务本身会持有克隆的句柄。
use tauri::Manager; // 用于获取 AppHandle 和访问状态
use std::sync::Arc; // 原子引用计数智能指针，用于共享状态
use rust_websocket_utils::message::WsMessage; // Import the WsMessage type from the utility crate

/// Tauri 命令：连接到云端 WebSocket 服务器。
///
/// 此异步命令负责启动与在 `url` 参数中指定 (或从配置中获取的默认) WebSocket 服务器的连接过程。
/// 它通过依赖注入的 `WebSocketClientService` 状态来执行实际的连接操作。
///
/// # 参数
/// - `_app_handle`: `AppHandle` - Tauri 应用的句柄。虽然在此命令的直接逻辑中可能未使用，
///   但它是 Tauri 状态注入机制所依赖的参数之一，确保 `State` 能够被正确解析。
///   `WebSocketClientService` 自身在创建时会接收并存储一个 `AppHandle` 的克隆，用于其内部操作 (如发送事件)。
/// - `state`: `State<'_, Arc<WebSocketClientService>>` - Tauri 托管状态，包含对共享的 `WebSocketClientService` 实例的原子引用计数智能指针。
///   通过此状态，命令可以访问并调用 `WebSocketClientService` 的方法。
/// - `url`: `Option<String>` - 可选的 WebSocket 服务器 URL 字符串。
///   - 如果提供了 `Some(String)`，则使用此 URL 进行连接。
///   - 如果为 `None`，则命令会尝试从 `WsClientConfig` 加载默认的 `cloud_ws_url` 配置项作为连接目标。
///
/// # 返回值
/// - `Result<(), String>`:
///   - `Ok(())`: 如果连接过程已成功启动 (注意：这并不意味着连接已完全建立，而是指连接的异步任务已开始执行)，则返回 `Ok`。
///   - `Err(String)`: 如果在准备连接 (例如，加载配置失败) 或尝试启动连接时发生错误，则返回包含本地化错误描述的 `Err`。
#[tauri::command] // 将此 Rust 函数标记为一个可从前端调用的 Tauri 命令。
pub async fn connect_to_cloud(
    _app_handle: AppHandle, // AppHandle 参数主要用于 Tauri 的状态注入机制，服务本身会持有自己的句柄副本。
    ws_service: State<'_, Arc<WebSocketClientService>>,
    url: String,
) -> Result<(), String> {
    info!(
        "接收到 Tauri 命令 'connect_to_cloud'，目标 URL: {}",
        url
    );
    match ws_service.connect(&url).await {
        Ok(_) => {
            info!("WebSocket 连接过程已成功启动。");
            Ok(())
        }
        Err(e) => {
            error!("启动 WebSocket 连接过程失败: {}", e);
            Err(format!("Failed to start WebSocket connection: {}", e))
        }
    }
}

/// Tauri 命令：从云端 WebSocket 服务器断开连接。
///
/// 此异步命令负责触发与云端 WebSocket 服务器的断开连接过程。
/// 它通过依赖注入的 `WebSocketClientService` 状态来执行实际的断开操作。
///
/// # 参数
/// - `state`: `State<'_, Arc<WebSocketClientService>>` - Tauri 托管状态，包含对共享的 `WebSocketClientService` 实例的引用。
///
/// # 返回值
/// - `Result<(), String>`:
///   - `Ok(())`: 如果断开连接的请求已成功处理 (例如，发送了关闭信号或清理了相关资源)，则返回 `Ok`。
///   - `Err(String)`: 如果在尝试断开连接时发生错误，则返回包含本地化错误描述的 `Err`。
#[tauri::command]
pub async fn disconnect_from_cloud(
    ws_service: State<'_, Arc<WebSocketClientService>>,
) -> Result<(), String> {
    info!("接收到 Tauri 命令 'disconnect_from_cloud'");
    match ws_service.disconnect().await {
        Ok(_) => {
            info!("WebSocket 断开操作已成功启动或已断开。");
            Ok(())
        }
        Err(e) => {
            error!("启动 WebSocket 断开操作失败: {}", e);
            Err(format!("Failed to start WebSocket disconnection: {}", e))
        }
    }
}

/// Tauri 命令：检查当前 WebSocket 的连接状态。
///
/// 此异步命令用于查询并返回当前 `SatControlCenter` 与云端 WebSocket 服务器的连接状态 (已连接或未连接)。
///
/// # 参数
/// - `state`: `State<'_, Arc<WebSocketClientService>>` - Tauri 托管状态，包含对共享的 `WebSocketClientService` 实例的引用。
///
/// # 返回值
/// - `Result<bool, String>`:
///   - `Ok(bool)`: 成功获取连接状态。`true` 表示已连接，`false` 表示未连接。
///   - `Err(String)`: （理论上此命令不应轻易失败，除非状态管理本身出现问题）如果获取状态时发生内部错误，则返回 `Err`。
#[tauri::command]
pub async fn check_ws_connection_status(
    state: State<'_, Arc<WebSocketClientService>>, // 注入 WebSocket 客户端服务状态
) -> Result<bool, String> {
    info!("Tauri 命令 'check_ws_connection_status' (检查WebSocket连接状态) 被调用。");
    let ws_service = state.inner().clone(); // 获取服务实例
    let is_connected = ws_service.is_connected().await; // 调用服务方法检查连接状态
    info!("命令 'check_ws_connection_status': 当前连接状态为: {}", if is_connected { "已连接" } else { "未连接" });
    Ok(is_connected) // 直接返回布尔值表示连接状态
}

// --- P2.2.1: Echo (回声测试) 相关命令 ---

/// Tauri 命令：向云端 WebSocket 服务发送一个 Echo (回声测试) 请求。
///
/// # Arguments
/// * `content` - 需要发送的回声内容。
/// * `ws_service` - 通过 Tauri 状态管理的 `WebSocketClientService` 实例。
///
/// # Returns
/// * `Result<(), String>` - 成功时返回 `Ok(())`，失败时返回错误描述字符串。
#[tauri::command]
pub async fn send_ws_echo(
    content: String,
    ws_service: State<'_, Arc<WebSocketClientService>>,
) -> Result<(), String> {
    info!("接收到 Tauri 命令 'send_ws_echo'，内容：'{}'", content);
    if !ws_service.is_connected().await {
        let err_msg = "无法发送 Echo：WebSocket 未连接。请先连接。".to_string();
        warn!("{}", err_msg);
        return Err(err_msg);
    }

    let echo_payload = EchoPayload {
        content: content.clone(),
    };

    let payload_json = match serde_json::to_string(&echo_payload) {
        Ok(json) => json,
        Err(e) => {
            let err_msg = format!("序列化 EchoPayload 失败: {}", e);
            error!("{}", err_msg);
            return Err(err_msg);
        }
    };

    // Construct the message using rust_websocket_utils::message::WsMessage
    let ws_message = WsMessage {
        message_id: Uuid::new_v4().to_string(), // Convert Uuid to String
        message_type: common_models::ws_payloads::ECHO_MESSAGE_TYPE.to_string(), // Use type from common_models
        payload: payload_json,
        timestamp: Utc::now().timestamp_millis(), // Convert DateTime<Utc> to i64 milliseconds
    };

    match ws_service.send_ws_message(ws_message).await {
        Ok(_) => {
            info!("已成功通过 WebSocket 发送 Echo 请求，内容：'{}'", content);
            Ok(())
        }
        Err(e) => {
            error!("通过 WebSocket 发送 Echo 请求失败: {}", e);
            Err(format!("发送 Echo 失败: {}", e))
        }
    }
}

/// Tauri 命令：向云端注册客户端并关联一个特定的任务。
///
/// 此命令构建一个 `RegisterPayload`，其中包含用户提供的 `group_id` 和 `task_id`，
/// 并固定角色为 `ClientRole::ControlCenter`。然后通过 WebSocket 将其发送到云端。
/// 云端的响应 (成功/失败、伙伴状态更新、初始任务状态) 将通过 WebSocket 消息
/// 触发 `WebSocketClientService` 中的相应处理逻辑，并通过 Tauri 事件通知前端。
///
/// # Arguments
/// * `group_id` - 用户指定的调试组 ID。
/// * `task_id` - 用户指定的当前调试任务 ID。
/// * `ws_service` - 通过 Tauri 状态管理的 `WebSocketClientService` 实例。
///
/// # Returns
/// * `Result<(), String>` - 成功发送注册消息时返回 `Ok(())`，否则返回错误描述。
#[tauri::command]
pub async fn register_client_with_task(
    group_id: String,
    task_id: String,
    ws_service: State<'_, Arc<WebSocketClientService>>,
) -> Result<(), String> {
    info!(
        "接收到 Tauri 命令 'register_client_with_task'：组ID='{}', 任务ID='{}'",
        group_id, task_id
    );
    if !ws_service.is_connected().await {
        let err_msg = "无法注册：WebSocket 未连接。请先连接。".to_string();
        warn!("{}", err_msg);
        return Err(err_msg);
    }
    let register_payload = RegisterPayload {
        group_id: group_id.clone(), // Clone group_id for payload
        role: ClientRole::ControlCenter, // Control Center always uses this role
        task_id: task_id.clone(), // Clone task_id for payload
    };
    let payload_json = match serde_json::to_string(&register_payload) {
        Ok(json) => json,
        Err(e) => {
            let err_msg = format!("序列化 RegisterPayload 失败: {}", e);
            error!("{}", err_msg);
            return Err(err_msg);
        }
    };
    // Construct the message using rust_websocket_utils::message::WsMessage
    let ws_message = WsMessage {
        message_id: Uuid::new_v4().to_string(), // Convert Uuid to String
        message_type: common_models::ws_payloads::REGISTER_MESSAGE_TYPE.to_string(), // Use type from common_models
        payload: payload_json,
        timestamp: Utc::now().timestamp_millis(), // Convert DateTime<Utc> to i64 milliseconds
    };
    
    match ws_service.send_ws_message(ws_message).await {
         Ok(_) => {
            info!(
                "已成功通过 WebSocket 发送 Register 请求：组ID='{}', 任务ID='{}'",
                group_id, task_id
            );
            Ok(())
        }
        Err(e) => {
            error!(
                "通过 WebSocket 发送 Register 请求失败: {} (组ID='{}', 任务ID='{}')",
                e, group_id, task_id
            );
            Err(format!(
                "发送 Register 失败: {} (组ID='{}', 任务ID='{}')",
                e, group_id, task_id
            ))
        }
    }
}

// 占位注释，说明本模块当前已包含 P2.1.1 (连接/断开/状态检查) 和 P2.2.1 (Echo命令) 的核心实现。
// 未来可根据通用命令的需求扩展此文件。 