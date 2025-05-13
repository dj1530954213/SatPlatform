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
    WsMessage, // 这个是正确的 WsMessage
    EchoPayload, RegisterPayload,
    ECHO_MESSAGE_TYPE, REGISTER_MESSAGE_TYPE,
};
use chrono::Utc; // `chrono` crate，用于获取当前的 UTC 时间戳。
use uuid::Uuid; // `uuid` crate，用于生成唯一的 UUID (通用唯一标识符)，例如为 `WsMessage` 生成 `message_id`。
use tauri::AppHandle; // Tauri 应用句柄，提供对应用核心功能的访问，例如发送事件、管理窗口等。在此处主要用于Tauri的State注入机制，服务本身会持有克隆的句柄。
use tauri::Manager; // 用于获取 AppHandle 和访问状态
use std::sync::Arc; // 原子引用计数智能指针，用于共享状态

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

/// Tauri 命令：向云端 WebSocket 服务发送 Echo (回声) 消息。
///
/// 此异步命令用于测试与云端服务的双向通信是否畅通。它会构造一个包含指定 `content` (内容) 的
/// Echo (回声) 请求消息，并通过 `WebSocketClientService` 将其发送到已连接的云端服务器。
/// 云端服务器收到此 Echo 消息后，应将其原样返回。前端可以通过监听特定的事件
/// (例如，在 `event.rs` 中定义的 `ECHO_RESPONSE_EVENT`) 来接收这个回声响应。
///
/// # 参数
/// - `state`: `State<'_, Arc<WebSocketClientService>>` - Tauri 托管状态，包含对共享的 `WebSocketClientService` 实例的引用。
/// - `content`: `String` - 要包含在 Echo 消息中的文本内容。
///
/// # 返回值
/// - `Result<(), String>`:
///   - `Ok(())`: 如果 Echo 消息已成功序列化并交由 `WebSocketClientService` 发送，则返回 `Ok`。
///     这并不保证消息已到达服务器或服务器已响应，仅表示发送操作已成功启动。
///   - `Err(String)`: 如果在构造消息 (例如，序列化 `EchoPayload` 失败) 或尝试发送消息时发生错误，
///     则返回包含本地化错误描述的 `Err`。
#[tauri::command]
pub async fn send_ws_echo(
    ws_service: State<'_, Arc<WebSocketClientService>>,
    content: String,
) -> Result<(), String> {
    debug!(
        "接收到 Tauri 命令 'send_ws_echo'，内容: '{}'",
        content
    );
    if !ws_service.is_connected().await {
        let err_msg = "无法发送 Echo：WebSocket 未连接。".to_string();
        warn!("{}", err_msg);
        return Err(err_msg);
    }
    let echo_payload = EchoPayload { content };
    let payload_json = match serde_json::to_string(&echo_payload) {
        Ok(json) => json,
        Err(e) => {
            let err_msg = format!("序列化 EchoPayload 失败: {}", e);
            error!("{}", err_msg);
            return Err(err_msg);
        }
    };
    // **确保这里使用的是 common_models::WsMessage**
    let ws_message = WsMessage {
        message_id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        message_type: ECHO_MESSAGE_TYPE.to_string(),
        payload: payload_json,
    };
    // **调用 ws_service 的 send_ws_message**
    match ws_service.send_ws_message(ws_message).await { // 类型应该匹配
        Ok(_) => {
            debug!("Echo 消息已成功提交到发送队列。");
            Ok(())
        }
        Err(e) => {
            error!("提交 Echo 消息到发送队列失败: {}", e);
            Err(format!("Failed to send Echo message: {}", e))
        }
    }
}

/// Tauri 命令：注册客户端到云端并关联特定任务。
///
/// 当前端（Angular）用户发起注册操作时调用此命令。
/// 它负责构建注册负载，并通过 WebSocket 服务将注册消息发送到云端。
///
/// # Arguments
///
/// * `group_id` - 用户希望加入或创建的组的标识符。
/// * `task_id` - 用户希望关联的调试任务的唯一标识符。
/// * `ws_service` - 通过 Tauri 状态管理的 WebSocket 客户端服务实例。
///
/// # Returns
///
/// * `Result<(), String>` - 成功时返回 `Ok(())`，失败时返回包含错误信息的 `Err(String)`。
///   注意：这个结果仅表示尝试发送消息的操作是否成功启动，
///   实际的注册成功与否需要通过监听 `WsRegistrationStatusEvent` 事件来获知。
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
        group_id,
        role: ClientRole::ControlCenter,
        task_id,
    };
    let payload_json = match serde_json::to_string(&register_payload) {
        Ok(json) => json,
        Err(e) => {
            let err_msg = format!("序列化 RegisterPayload 失败: {}", e);
            error!("{}", err_msg);
            return Err(err_msg);
        }
    };
    // **确保这里使用的是 common_models::WsMessage**
    let ws_message = WsMessage {
        message_id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        message_type: REGISTER_MESSAGE_TYPE.to_string(),
        payload: payload_json,
    };
    // **调用 ws_service 的 send_ws_message**
    match ws_service.send_ws_message(ws_message).await { // 类型应该匹配
        Ok(_) => {
            info!("注册 ('Register') 消息已成功提交到发送队列。");
            Ok(())
        }
        Err(e) => {
            error!("提交注册 ('Register') 消息到发送队列失败: {}", e);
            Err(format!("Failed to send registration message: {}", e))
        }
    }
}

// 占位注释，说明本模块当前已包含 P2.1.1 (连接/断开/状态检查) 和 P2.2.1 (Echo命令) 的核心实现。
// 未来可根据通用命令的需求扩展此文件。 