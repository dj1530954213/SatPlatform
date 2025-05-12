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
use log::{info, error}; // `log` crate，用于在后端记录不同级别的日志信息 (信息、错误等)。
// use crate::ws_client::service::WsClientService; // 旧的或错误的导入路径，已被注释或将在后续清理中移除。
use crate::ws_client::WebSocketClientService; // 从 `ws_client` 模块导入核心的 WebSocket 客户端服务结构体。
use crate::config::WsClientConfig; // 从 `config` 模块导入 `WsClientConfig`，用于加载 WebSocket 客户端配置 (如默认URL)。
use std::sync::Arc; // `Arc` (原子引用计数)，`WebSocketClientService` 通过 `Arc` 实现共享所有权，并安全地在多线程/异步任务间传递。
use tauri::AppHandle; // Tauri 应用句柄，提供对应用核心功能的访问，例如发送事件、管理窗口等。在此处主要用于Tauri的State注入机制，服务本身会持有克隆的句柄。
use rust_websocket_utils::message::WsMessage; // 从 `rust_websocket_utils` 导入标准的 WebSocket 消息结构体。
use common_models; // 导入 `common_models` crate，用于访问共享的数据模型，例如 `EchoPayload` 和消息类型常量。
use chrono::Utc; // `chrono` crate，用于获取当前的 UTC 时间戳。
use uuid::Uuid; // `uuid` crate，用于生成唯一的 UUID (通用唯一标识符)，例如为 `WsMessage` 生成 `message_id`。
use tauri::Manager; // 用于获取 AppHandle 和访问状态
use common_models::{
    enums::ClientRole,
    ws_messages::WsMessage, // 假设 WsMessage 在 common_models::ws_messages 中定义
    ws_payloads::{RegisterPayload, REGISTER_MESSAGE_TYPE},
};
// 确保 WsRequest 和 AppState 的路径正确
use crate::ws_client::services::{AppState, WsRequest}; // 假设 WsService 实例在 AppState 中

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
    state: State<'_, Arc<WebSocketClientService>>, // 注入 WebSocket 客户端服务状态
    url: Option<String>, // 可选的 WebSocket 服务器 URL
) -> Result<(), String> {
    info!("Tauri 命令 'connect_to_cloud' (连接到云服务) 被调用，URL 参数: {:?}", url);

    // 从 Tauri State 中获取 WebSocketClientService 的克隆 (Arc 的克隆是轻量级的)
    let ws_service = state.inner().clone();

    // 确定最终要连接的 URL
    let connect_url = match url {
        Some(u) => u, // 如果前端提供了 URL，则使用该 URL
        None => { // 如果前端未提供 URL，则尝试从配置文件加载默认 URL
            info!("命令 'connect_to_cloud': URL 未在参数中提供，尝试从配置加载默认的云端 WebSocket URL...");
            // 注意：WsClientConfig::load() 可能会失败，这里简化处理，实际项目中应处理其 Result
            // 假设 WsClientConfig::load() 总是成功或 panic (不推荐)，或者其错误已在内部转为默认值
            let client_config = WsClientConfig::load(); // 加载客户端配置
            client_config.cloud_ws_url // 返回配置中的云端 WebSocket URL
        }
    };

    info!("命令 'connect_to_cloud': 最终连接目标 URL 为: {}", connect_url);

    // 调用 WebSocketClientService 的 connect 方法来启动连接过程
    // 注意：connect 方法是异步的，它会立即返回一个 Future，实际的连接建立在后台进行。
    match ws_service.connect(&connect_url).await { // `await` 在这里等待 connect 方法内部的异步操作完成一个阶段 (如任务创建)
        Ok(_) => {
            info!("命令 'connect_to_cloud': WebSocket 连接过程已成功启动。");
            Ok(()) // 表示连接过程启动成功
        }
        Err(e) => {
            error!("命令 'connect_to_cloud': 尝试启动 WebSocket 连接失败: {}", e);
            Err(format!("连接到云端服务失败，请检查网络或配置。错误详情: {}", e)) // 向前端返回格式化的错误信息
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
    state: State<'_, Arc<WebSocketClientService>>, // 注入 WebSocket 客户端服务状态
) -> Result<(), String> {
    info!("Tauri 命令 'disconnect_from_cloud' (从云服务断开) 被调用。");
    let ws_service = state.inner().clone(); // 获取服务实例

    // 调用 WebSocketClientService 的 disconnect 方法
    match ws_service.disconnect().await {
        Ok(_) => {
            info!("命令 'disconnect_from_cloud': 断开连接请求已成功处理。");
            Ok(())
        }
        Err(e) => {
            error!("命令 'disconnect_from_cloud': 尝试断开 WebSocket 连接失败: {}", e.to_string());
            Err(format!("断开与云端服务的连接时发生错误: {}", e.to_string()))
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
    state: State<'_, Arc<WebSocketClientService>>, // 注入 WebSocket 客户端服务状态
    content: String, // 要发送的回声内容
) -> Result<(), String> {
    info!("Tauri 命令 'send_ws_echo' (发送WebSocket回声消息) 被调用, 内容: \"{}\"", content);
    let ws_service = state.inner().clone(); // 获取服务实例

    // 步骤 1: 构建 `EchoPayload` (回声消息负载)
    // `EchoPayload` 是在 `common_models::ws_payloads` 中定义的共享数据结构。
    let echo_payload = common_models::ws_payloads::EchoPayload { content }; // 使用传入的 content 创建负载

    // 步骤 2: 将 `EchoPayload` 序列化为 JSON 字符串
    // WebSocket 消息的 `payload` 字段通常是 JSON 字符串。
    let payload_json = match serde_json::to_string(&echo_payload) {
        Ok(json_string) => json_string, // 序列化成功，得到 JSON 字符串
        Err(e) => {
            let error_message = format!("命令 'send_ws_echo': 序列化 EchoPayload (回声消息负载) 失败: {}", e);
            error!("{}", error_message); // 记录详细错误日志
            return Err(error_message); // 向前端返回错误信息
        }
    };

    // 步骤 3: 构建 `WsMessage` (标准 WebSocket 消息)
    // `WsMessage` 是在 `rust_websocket_utils` 中定义的标准消息结构，用于封装所有通过 WebSocket 发送的数据。
    let ws_message = WsMessage {
        message_id: Uuid::new_v4().to_string(), // 为消息生成一个唯一的 ID
        timestamp: Utc::now().timestamp_millis(), // 记录当前 UTC 时间戳 (毫秒)
        message_type: common_models::ws_payloads::ECHO_MESSAGE_TYPE.to_string(), // 设置消息类型为 Echo
        payload: payload_json, // 将序列化后的 EchoPayload JSON 字符串作为负载
    };
    info!("命令 'send_ws_echo': 已构建 WsMessage (WebSocket消息), 消息ID: {}, 类型: {}", ws_message.message_id, ws_message.message_type);

    // 步骤 4: 通过 WebSocketClientService 发送 `WsMessage`
    match ws_service.send_ws_message(ws_message).await {
        Ok(_) => {
            info!("命令 'send_ws_echo': Echo (回声) 消息已成功通过 WebSocketClientService 提交发送。");
            Ok(()) // 表示发送操作成功启动
        }
        Err(e) => {
            let error_message = format!("命令 'send_ws_echo': 发送 Echo (回声) 消息时发生错误: {}", e);
            error!("{}", error_message); // 记录详细错误日志
            Err(error_message) // 向前端返回错误信息
        }
    }
}

/// Tauri 命令，用于客户端向云端发起注册请求，并关联一个特定的任务。
///
/// # Arguments
/// * `app_handle` - Tauri 应用句柄，用于访问共享状态（如 WsService）。
/// * `group_id` - 用户希望加入或创建的调试组的ID。
/// * `task_id` - 当前要进行的调试任务的唯一标识。
///
/// # Returns
/// * `Result<(), String>` - 操作成功则返回 Ok(()), 失败则返回包含错误信息的 Err。
#[tauri::command]
pub async fn register_client_with_task(
    app_handle: tauri::AppHandle,
    group_id: String,
    task_id: String,
) -> Result<(), String> {
    info!(
        "Tauri 命令 'register_client_with_task' 被调用, group_id: {}, task_id: {}",
        group_id, task_id
    );

    // 构建 RegisterPayload
    let register_payload = RegisterPayload {
        group_id: group_id.clone(),
        role: ClientRole::ControlCenter, // 对于 SatControlCenter，角色固定为 ControlCenter
        task_id: task_id.clone(),
    };

    // 序列化 RegisterPayload 为 JSON 字符串
    let serialized_payload = match serde_json::to_string(&register_payload) {
        Ok(json_str) => json_str,
        Err(e) => {
            error!("序列化 RegisterPayload 失败: {}", e);
            return Err(format!("构建注册请求失败 (序列化错误): {}", e));
        }
    };

    // 创建 WsMessage
    // 假设 WsMessage 结构体包含 message_type 和 payload。
    // 如果 WsMessage 还需要 correlation_id 或 timestamp，需要在这里生成并填充。
    let ws_message = WsMessage {
        message_type: REGISTER_MESSAGE_TYPE.to_string(),
        payload: serialized_payload,
        // correlation_id: Some(uuid::Uuid::new_v4().to_string()), // 示例：如果需要
        // timestamp: Some(chrono::Utc::now().to_rfc3339()),      // 示例：如果需要
    };

    // 从 AppState 获取 WsService 实例的引用
    let app_state = match app_handle.try_state::<AppState>() {
        Some(state) => state,
        None => {
            error!("无法从 Tauri AppHandle 获取 AppState。WsService 可能未正确初始化。");
            return Err("内部服务器错误: WebSocket 服务状态不可用".to_string());
        }
    };

    // 在调用 submit_request 之前，先调用 WsService 的 set_current_task_context
    // 这样 WsService 在处理 RegisterResponse 时，可以知道 task_id
    app_state.ws_service.set_current_task_context(group_id.clone(), task_id.clone()).await;
    info!("WsService 上下文已设置: group_id={}, task_id={}", group_id, task_id);


    // 通过 WsService 发送消息
    match app_state.ws_service.submit_request(WsRequest::SendMessage(ws_message)).await {
        Ok(_) => {
            info!(
                "已请求 WsService 发送 'Register' 消息, group_id: {}, task_id: {}",
                group_id, task_id
            );
            Ok(())
        }
        Err(e) => {
            error!("请求 WsService 发送 'Register' 消息失败: {}", e);
            Err(format!("发送注册消息失败: {}", e))
        }
    }
}

// 占位注释，说明本模块当前已包含 P2.1.1 (连接/断开/状态检查) 和 P2.2.1 (Echo命令) 的核心实现。
// 未来可根据通用命令的需求扩展此文件。 