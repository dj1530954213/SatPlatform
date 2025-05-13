// SatOnSiteMobile/src-tauri/src/commands/general_cmds.rs

//! `SatOnSiteMobile` (现场端) 应用的通用 Tauri 命令模块。
//!
//! 此模块包含由前端 UI 调用，用于与后端服务（如 WebSocket 客户端服务）交互的命令。
//! 主要负责处理连接建立、断开、状态检查以及基础消息（如 Echo）的发送。

use tauri::State;
use log::{info, error};
use crate::ws_client::service::{WebSocketClientService};
use crate::config::WsClientConfig; // 注意：WsClientConfig 可能需要在 SatOnSiteMobile 中定义或调整
use std::sync::Arc;
use tauri::AppHandle;
use rust_websocket_utils::message::WsMessage;
use common_models;
use chrono::Utc;
use uuid::Uuid;
use common_models::ws_payloads::{RegisterPayload, REGISTER_MESSAGE_TYPE};
use common_models::enums::ClientRole;
use tauri::Manager;

/// 连接到云端 WebSocket 服务器。
///
/// 前端通过此命令请求与云端建立 WebSocket 连接。
/// 如果提供了 `url` 参数，则使用该 URL；否则，尝试从配置文件加载默认 URL。
///
/// # 参数
/// * `_app_handle`: Tauri 应用句柄 (当前未使用，但保留以符合 Tauri 命令签名规范)。
/// * `state`: `WebSocketClientService` 的共享状态，通过 Tauri 的状态管理注入。
/// * `url`: 可选的 WebSocket 服务器 URL 字符串。如果为 `None`，则会尝试加载配置中的默认值。
///
/// # 返回
/// * `Result<(), String>`: 连接过程启动成功则返回 `Ok(())`，否则返回包含错误描述的 `Err(String)`。
///   注意：成功返回仅表示连接尝试已开始，实际连接状态通过事件 (`WsConnectionStatusEvent`) 异步通知。
#[tauri::command]
pub async fn connect_to_cloud(
    _app_handle: AppHandle, // _app_handle 未使用，添加下划线前缀
    state: State<'_, Arc<WebSocketClientService>>,
    url: Option<String>,
) -> Result<(), String> {
    info!("[SatOnSiteMobile] Tauri 命令 'connect_to_cloud' 被调用，URL 参数: {:?}", url);

    let ws_service = state.inner().clone();

    let connect_url = match url {
        Some(u) => u,
        None => {
            info!("[SatOnSiteMobile] URL 未在命令参数中提供，尝试从配置文件加载默认 URL...");
            // 注意: SatOnSiteMobile 的配置加载逻辑可能与 SatControlCenter 不同。
            // 此处假设 WsClientConfig::load() 对于现场端是有效的，或者有特定的默认 URL。
            match WsClientConfig::load() {
                 Ok(client_config) => client_config.cloud_ws_url,
                 Err(e) => {
                    let err_msg = format!("[SatOnSiteMobile] 加载客户端配置 (WsClientConfig) 失败: {}。请在调用时提供明确的 URL，或确保配置文件存在且有效。", e);
                    error!("{}", err_msg);
                    return Err(err_msg); // 返回中文错误信息
                 }
            }
        }
    };

    info!("[SatOnSiteMobile] 最终确定连接目标 URL: {}", connect_url);

    match ws_service.connect(&connect_url).await {
        Ok(_) => {
            info!("[SatOnSiteMobile] 命令 'connect_to_cloud': WebSocket 连接过程已成功启动。");
            Ok(())
        }
        Err(e) => {
            error!("[SatOnSiteMobile] 命令 'connect_to_cloud': 启动连接过程失败: {}", e);
            Err(format!("连接到云服务失败: {}", e)) // 错误信息保持一定技术性，但前缀指明操作
        }
    }
}

/// 从云端 WebSocket 服务器断开连接。
///
/// 前端通过此命令请求断开当前的 WebSocket 连接。
///
/// # 参数
/// * `state`: `WebSocketClientService` 的共享状态。
///
/// # 返回
/// * `Result<(), String>`: 断开过程处理成功则返回 `Ok(())`，否则返回包含错误描述的 `Err(String)`。
#[tauri::command]
pub async fn disconnect_from_cloud(
    state: State<'_, Arc<WebSocketClientService>>,
) -> Result<(), String> {
    info!("[SatOnSiteMobile] Tauri 命令 'disconnect_from_cloud' 被调用。");
    let ws_service = state.inner().clone();
    match ws_service.disconnect().await {
        Ok(_) => {
            info!("[SatOnSiteMobile] 命令 'disconnect_from_cloud': 断开连接请求已成功处理。");
            Ok(())
        }
        Err(e) => {
            error!("[SatOnSiteMobile] 命令 'disconnect_from_cloud': 处理断开连接请求失败: {}", e);
            Err(format!("断开连接失败: {}", e))
        }
    }
}

/// 检查当前 WebSocket 的连接状态。
///
/// # 参数
/// * `state`: `WebSocketClientService` 的共享状态。
///
/// # 返回
/// * `Result<bool, String>`: 成功获取状态则返回 `Ok(true)` (已连接) 或 `Ok(false)` (未连接)。
///   理论上此命令不应失败，但为保持一致性返回 `Result`。
#[tauri::command]
pub async fn check_ws_connection_status(
    state: State<'_, Arc<WebSocketClientService>>,
) -> Result<bool, String> {
    info!("[SatOnSiteMobile] Tauri 命令 'check_ws_connection_status' 被调用。");
    let ws_service = state.inner().clone();
    let is_connected = ws_service.is_connected().await;
    info!("[SatOnSiteMobile] 当前 WebSocket 连接状态: {}", if is_connected { "已连接" } else { "未连接" });
    Ok(is_connected)
}

/// 发送 Echo (回声) 消息到云端 WebSocket 服务器。
///
/// 用于测试 WebSocket 连接是否通畅以及消息收发是否正常。
///
/// # 参数
/// * `state`: `WebSocketClientService` 的共享状态。
/// * `content`: 要发送的 Echo 内容字符串。
///
/// # 返回
/// * `Result<(), String>`: Echo 消息成功发送则返回 `Ok(())`，否则返回包含错误描述的 `Err(String)`。
#[tauri::command]
pub async fn send_ws_echo(
    state: State<'_, Arc<WebSocketClientService>>,
    content: String,
) -> Result<(), String> {
    info!("[SatOnSiteMobile] Tauri 命令 'send_ws_echo' 被调用, 发送内容: {:?}", content);
    let ws_service = state.inner().clone();

    // 构建 EchoPayload
    let echo_payload = common_models::ws_payloads::EchoPayload { content: content.clone() }; // 克隆 content 用于 payload

    // 序列化 EchoPayload 为 JSON 字符串
    let payload_json = match serde_json::to_string(&echo_payload) {
        Ok(json) => json,
        Err(e) => {
            let err_msg = format!("序列化 EchoPayload 失败: {}", e);
            error!("[SatOnSiteMobile] {}", err_msg);
            return Err(err_msg); // 返回中文错误
        }
    };

    // 构建 WsMessage
    let ws_message = WsMessage {
        message_id: Uuid::new_v4().to_string(), // 生成唯一消息 ID
        timestamp: Utc::now().timestamp_millis(), // 设置当前时间戳
        message_type: common_models::ws_payloads::ECHO_MESSAGE_TYPE.to_string(),
        payload: payload_json,
    };
    // 注意: WsMessage::new() 是一个更简洁的构造方式，如果已在 common_models 中实现。
    // let ws_message = WsMessage::new(
    //     common_models::ws_payloads::ECHO_MESSAGE_TYPE.to_string(),
    //     payload_json,
    //     None, None, None // client_id, group_id, target_client_id 通常由服务端处理或在更上层逻辑决定
    // );


    match ws_service.send_ws_message(ws_message).await {
        Ok(_) => {
            info!("[SatOnSiteMobile] Echo 消息 (内容: {:?}) 已成功通过 WebSocket 服务发送。", content);
            Ok(())
        }
        Err(e) => {
            let err_msg = format!("发送 Echo 消息失败: {}", e);
            error!("[SatOnSiteMobile] {}", err_msg);
            Err(err_msg) // 返回中文错误
        }
    }
}

/// 向云端 WebSocket 服务器注册客户端并关联到指定的任务组。
///
/// 此命令用于客户端（如此处的现场端）向云端表明身份、期望加入的组以及关联的调试任务。
///
/// # 参数
/// * `app_handle`: Tauri 应用句柄。
/// * `group_id`: 客户端希望加入或创建的调试组的唯一标识符。
/// * `task_id`: 与此调试会话关联的具体任务的唯一标识符。
///
/// # 返回
/// * `Result<(), String>`: 注册消息成功发送则返回 `Ok(())`，否则返回包含错误描述的 `Err(String)`。
///   实际的注册成功与否通过后续的 "RegisterResponse" 事件进行通知。
#[tauri::command]
pub async fn register_client_with_task(
    app_handle: tauri::AppHandle,
    group_id: String,
    task_id: String,
) -> Result<(), String> {
    info!(
        "Tauri 命令 'register_client_with_task' (SatOnSiteMobile) 被调用, group_id: {}, task_id: {}",
        group_id, task_id
    );

    let register_payload = RegisterPayload {
        group_id: group_id.clone(),
        role: ClientRole::OnSiteMobile, // <--- 角色设置为 OnSiteMobile
        task_id: task_id.clone(),
    };

    let serialized_payload = match serde_json::to_string(&register_payload) {
        Ok(json_str) => json_str,
        Err(e) => {
            error!("序列化 RegisterPayload (SatOnSiteMobile) 失败: {}", e);
            return Err(format!("构建注册请求失败: {}", e));
        }
    };

    // 构建完整的 WsMessage，包含 message_id 和 timestamp
    let ws_message = WsMessage {
        message_id: Uuid::new_v4().to_string(),
        timestamp: Utc::now().timestamp_millis(),
        message_type: REGISTER_MESSAGE_TYPE.to_string(),
        payload: serialized_payload,
    };

    // 获取 WsClientService 实例
    let ws_service = match app_handle.try_state::<Arc<WebSocketClientService>>() {
        Some(state) => state.inner().clone(),
        None => {
            error!("无法从 Tauri AppHandle 获取 WebSocketClientService 状态 (SatOnSiteMobile)。");
            return Err("内部服务器错误: WebSocket 服务状态不可用".to_string());
        }
    };

    match ws_service.send_ws_message(ws_message).await {
        Ok(_) => {
            info!(
                "已通过 WsService 发送 'Register' 消息 (SatOnSiteMobile), group_id: {}, task_id: {}",
                group_id, task_id
            );
            Ok(())
        }
        Err(e) => {
            error!("通过 WsService 发送 'Register' 消息 (SatOnSiteMobile) 失败: {}", e);
            Err(format!("发送注册消息失败: {}", e))
        }
    }
}
// 后续根据开发步骤 (如 P4.2.1) 可能在此处或新的 `*_cmds.rs` 文件中添加更多业务相关命令。
// 例如: send_update_pre_check_item_command, send_single_test_step_feedback_command 等。 