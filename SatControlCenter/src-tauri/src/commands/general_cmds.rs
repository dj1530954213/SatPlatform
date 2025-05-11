// SatControlCenter/src-tauri/src/commands/general_cmds.rs

//! 处理通用 Tauri 命令，例如连接 WebSocket 等。

use tauri::State;
use log::{info, error};
// use crate::ws_client::service::WsClientService; // 错误导入，将被删除
use crate::ws_client::WebSocketClientService;
use crate::config::WsClientConfig;
use std::sync::Arc;
use tauri::AppHandle;
use rust_websocket_utils::message::WsMessage;
use common_models;
use chrono::Utc;
use uuid::Uuid;

/// 连接到云端WebSocket服务器
/// 
/// # 参数
/// * `url` - WebSocket服务器URL
/// * `state` - 应用状态，包含WebSocket客户端服务实例
/// 
/// # 返回
/// * `Result<(), String>` - 连接结果
#[tauri::command]
pub async fn connect_to_cloud(
    _app_handle: AppHandle, // app_handle is captured by the service, keep param for now if Tauri requires it for state injection, though service won't use it directly from here.
    state: State<'_, Arc<WebSocketClientService>>,
    url: Option<String>,
) -> Result<(), String> {
    info!("Tauri 命令 'connect_to_cloud' 被调用，URL 参数: {:?}", url);

    let ws_service = state.inner().clone();

    let connect_url = match url {
        Some(u) => u,
        None => {
            info!("URL 未在命令中提供，尝试从配置加载默认 URL...");
            let client_config = WsClientConfig::load();
            client_config.cloud_ws_url
        }
    };

    info!("最终连接目标 URL: {}", connect_url);

    // 移除 app_handle 作为参数传递给 ws_service.connect
    match ws_service.connect(&connect_url).await {
        Ok(_) => {
            info!("命令 'connect_to_cloud': WebSocket 连接过程已启动。");
            Ok(())
        }
        Err(e) => {
            error!("命令 'connect_to_cloud': 连接失败: {}", e);
            Err(format!("连接到云服务失败: {}", e))
        }
    }
}

#[tauri::command]
pub async fn disconnect_from_cloud(
    state: State<'_, Arc<WebSocketClientService>>,
) -> Result<(), String> {
    info!("Tauri 命令 'disconnect_from_cloud' 被调用.");
    let ws_service = state.inner().clone();
    match ws_service.disconnect().await {
        Ok(_) => {
            info!("命令 'disconnect_from_cloud': 断开请求已处理。");
            Ok(())
        }
        Err(e) => {
            error!("命令 'disconnect_from_cloud': 断开失败: {}", e.to_string());
            Err(format!("断开连接失败: {}", e.to_string()))
        }
    }
}

#[tauri::command]
pub async fn check_ws_connection_status(
    state: State<'_, Arc<WebSocketClientService>>,
) -> Result<bool, String> {
    info!("Tauri 命令 'check_ws_connection_status' 被调用.");
    let ws_service = state.inner().clone();
    Ok(ws_service.is_connected().await)
}

// --- P2.2.1: Echo 命令 ---
#[tauri::command]
pub async fn send_ws_echo(
    state: State<'_, Arc<WebSocketClientService>>,
    content: String,
) -> Result<(), String> {
    info!("Tauri 命令 'send_ws_echo' 被调用, content: \"{}\"", content);
    let ws_service = state.inner().clone();

    // 1. 构建 EchoPayload
    let echo_payload = common_models::ws_payloads::EchoPayload { content };

    // 2. 将 EchoPayload 序列化为 JSON 字符串
    let payload_json = match serde_json::to_string(&echo_payload) {
        Ok(json) => json,
        Err(e) => {
            let err_msg = format!("序列化 EchoPayload 失败: {}", e);
            error!("{}", err_msg);
            return Err(err_msg);
        }
    };

    // 3. 构建 WsMessage
    let ws_message = WsMessage {
        message_id: Uuid::new_v4().to_string(),
        timestamp: Utc::now().timestamp_millis(),
        message_type: common_models::ws_payloads::ECHO_MESSAGE_TYPE.to_string(),
        payload: payload_json,
    };

    // 4. 发送 WsMessage
    match ws_service.send_ws_message(ws_message).await {
        Ok(_) => {
            info!("Echo 消息已成功发送到 WebSocket 服务.");
            Ok(())
        }
        Err(e) => {
            let err_msg = format!("发送 Echo 消息失败: {}", e);
            error!("{}", err_msg);
            Err(err_msg)
        }
    }
}

// 暂时为空，后续会根据 P2.1.1, P2.2.1 等步骤添加具体命令实现。 