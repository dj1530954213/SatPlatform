// SatControlCenter/src-tauri/src/ws_client/service.rs

//! `SatControlCenter` (中心端) 应用的 WebSocket 客户端服务模块。
//!
//! 该模块负责管理与云端 `SatCloudService` 的 WebSocket 连接，
//! 包括连接建立、断开、消息收发、状态同步以及心跳维持。

use anyhow::Result;
use log::{debug, error, info, warn};
use rust_websocket_utils::client::transport;
use rust_websocket_utils::client::transport::ClientWsStream;
use rust_websocket_utils::message::WsMessage;
use rust_websocket_utils::error::WsError;
use tauri::{AppHandle, Emitter};
use tokio::sync::{RwLock};
use tokio::sync::Mutex as TokioMutex;
use std::sync::Arc;
use futures_util::stream::{SplitSink};
use tokio_tungstenite::tungstenite::protocol::Message as TungsteniteMessage;
use futures_util::SinkExt;
use chrono::{Utc, DateTime};
use std::time::Duration;
use uuid::Uuid;

use crate::event::{
    WS_CONNECTION_STATUS_EVENT, WsConnectionStatusEvent, ECHO_RESPONSE_EVENT, EchoResponseEventPayload,
    WS_REGISTRATION_STATUS_EVENT, WsRegistrationStatusEventPayload,
    WS_PARTNER_STATUS_EVENT, WsPartnerStatusEventPayload,
    LOCAL_TASK_STATE_UPDATED_EVENT, LocalTaskStateUpdatedEventPayload,
    WS_SERVER_ERROR_EVENT, WsServerErrorEventPayload,
};
use common_models::{
    self,
    ws_payloads::{
        PING_MESSAGE_TYPE, PONG_MESSAGE_TYPE, PingPayload,
        REGISTER_RESPONSE_MESSAGE_TYPE, RegisterResponsePayload,
        PARTNER_STATUS_UPDATE_MESSAGE_TYPE, PartnerStatusPayload,
        TASK_STATE_UPDATE_MESSAGE_TYPE,
        ERROR_RESPONSE_MESSAGE_TYPE, ErrorResponsePayload,
        ECHO_MESSAGE_TYPE, EchoPayload,
    },
    TaskDebugState,
};

// 心跳机制相关常量
/// 心跳发送间隔，单位：秒。
const HEARTBEAT_INTERVAL_SECONDS: u64 = 30;
/// 等待 Pong 回复的超时时间，单位：秒。
/// 注意：此值应小于 `HEARTBEAT_INTERVAL_SECONDS`，以确保在下一个心跳周期开始前能检测到超时。
const PONG_TIMEOUT_SECONDS: u64 = 10;

/// WebSocket 客户端服务。
///
/// 封装了与云端 WebSocket 服务交互的所有逻辑，包括连接管理、
/// 消息发送与接收、心跳维持以及状态事件的发射。
#[derive(Debug)]
pub struct WebSocketClientService {
    /// WebSocket 消息发送通道 (Sink 部分)。
    ///
    /// `SplitSink` 用于向 WebSocket 连接异步发送消息。
    /// 使用 `Arc<TokioMutex<Option<...>>>` 以允许在异步任务间共享和修改。
    /// `Option` 表示连接可能尚未建立或已断开。
    ws_send_channel: Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
    /// 云端分配给此客户端的唯一标识符 (UUID)。
    ///
    /// 在成功连接并注册后，由云端分配 (通过 RegisterResponse 确认)。
    /// 使用 `Arc<RwLock<Option<Uuid>>>` 以便在异步回调中安全地更新和读取。
    cloud_assigned_client_id: Arc<RwLock<Option<Uuid>>>,
    /// Tauri 应用句柄。
    ///
    /// 用于向前端发送事件和管理应用状态。
    app_handle: AppHandle,
    /// 当前 WebSocket 连接状态。
    ///
    /// `true` 表示已连接，`false` 表示未连接或已断开。
    /// 使用 `Arc<RwLock<...>>` 以便在异步回调中安全地更新和读取。
    is_connected_status: Arc<RwLock<bool>>,
    /// WebSocket 连接处理任务的句柄。
    ///
    /// `tokio::task::JoinHandle` 用于管理 `connect_client` 返回的连接处理任务的生命周期。
    /// `Option` 表示任务可能尚未启动或已结束。
    connection_task_handle: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
    /// 心跳任务的句柄。
    ///
    /// 用于管理定期发送 Ping 消息的后台任务。
    heartbeat_task_handle: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
    /// 最后一次收到 Pong 消息的时间戳。
    ///
    /// 用于心跳机制中检测连接是否仍然活跃。
    last_pong_received_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    /// 本地缓存的、从云端同步过来的权威任务状态。
    ///
    /// 当收到云端的 "TaskStateUpdate" 消息时，此状态会被更新。
    /// 前端可以通过监听 `LocalTaskStateUpdatedEvent` 事件来获取最新的状态。
    local_task_state_cache: Arc<RwLock<Option<TaskDebugState>>>,
}

impl WebSocketClientService {
    /// 创建 `WebSocketClientService` 的新实例。
    ///
    /// # 参数
    ///
    /// * `app_handle` - Tauri 应用的句柄，用于事件发射和应用管理。
    ///
    /// # 返回
    ///
    /// 返回一个新的 `WebSocketClientService` 实例。
    pub fn new(app_handle: AppHandle) -> Self {
        info!("[SatControlCenter] WebSocketClientService: 正在初始化...");
        Self {
            ws_send_channel: Arc::new(TokioMutex::new(None)),
            cloud_assigned_client_id: Arc::new(RwLock::new(None)),
            app_handle,
            is_connected_status: Arc::new(RwLock::new(false)),
            connection_task_handle: Arc::new(TokioMutex::new(None)),
            heartbeat_task_handle: Arc::new(TokioMutex::new(None)),
            last_pong_received_at: Arc::new(RwLock::new(None)),
            local_task_state_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// 尝试连接到指定的 WebSocket 服务器 URL。
    ///
    /// 此方法会启动一个后台异步任务来处理 WebSocket 连接的整个生命周期，
    /// 包括建立连接、接收消息、处理心跳、以及处理断开或错误。
    /// 状态变更会通过 Tauri 事件通知前端。
    ///
    /// # 参数
    /// * `url_str`: 要连接的 WebSocket 服务器 URL 字符串。
    ///
    /// # 返回
    /// * `Result<(), String>`: 如果连接启动过程成功（即后台任务已安排），则返回 `Ok(())`。
    ///   如果 URL 无效或在启动连接任务时发生即时错误，则返回 `Err(String)`。
    ///   注意：实际的连接成功或失败将通过 `WS_CONNECTION_STATUS_EVENT` 事件异步通知。
    pub async fn connect(&self, url_str: &str) -> Result<(), String> {
        info!("[SatControlCenter] WebSocketClientService::connect 调用，目标 URL: {}", url_str);

        // 克隆需要在新任务中使用的 Arc 引用
        let app_handle_clone = self.app_handle.clone();
        let ws_send_channel_clone = self.ws_send_channel.clone();
        let cloud_assigned_client_id_clone = self.cloud_assigned_client_id.clone();
        let is_connected_status_clone = self.is_connected_status.clone();
        let connection_task_handle_clone = self.connection_task_handle.clone();
        let heartbeat_task_handle_clone = self.heartbeat_task_handle.clone();
        let last_pong_received_at_clone = self.last_pong_received_at.clone();
        let local_task_state_cache_clone = self.local_task_state_cache.clone();
        let url_string = url_str.to_string();

        // 如果当前已有连接任务在运行，先尝试取消它
        let mut conn_task_guard = connection_task_handle_clone.lock().await;
        if let Some(handle) = conn_task_guard.take() {
            info!("[SatControlCenter] 检测到之前的连接任务正在运行，正在尝试取消...");
            handle.abort(); // 请求取消任务
            match handle.await { // 等待任务结束（即使是被取消的）
                Ok(_) => info!("[SatControlCenter] 之前的连接任务已成功取消或完成。"),
                Err(e) if e.is_cancelled() => info!("[SatControlCenter] 之前的连接任务已被取消。"),
                Err(e) => warn!("[SatControlCenter] 等待之前的连接任务结束时发生错误: {:?}", e),
            }
            info!("[SatControlCenter] 已清理之前的连接任务句柄。");
            // 清理相关状态
            *is_connected_status_clone.write().await = false;
            *ws_send_channel_clone.lock().await = None;
            *cloud_assigned_client_id_clone.write().await = None;
            // 心跳任务句柄也应停止
            let mut hb_task_guard = heartbeat_task_handle_clone.lock().await;
            if let Some(hb_handle) = hb_task_guard.take() {
                 hb_handle.abort();
                 info!("[SatControlCenter] 已请求取消相关的心跳任务。");
            }
            *last_pong_received_at_clone.write().await = None;
        }
        drop(conn_task_guard); // 释放锁

        // 启动新的异步任务来处理连接生命周期
        let connection_task = tokio::spawn(async move {
            let mut connection_attempt_successful = false;
            let mut final_error_message: Option<String> = None;
            let mut heartbeat_join_handle: Option<tokio::task::JoinHandle<()>> = None;

            // --- 连接阶段 ---
            info!("[SatControlCenter] (连接任务) 开始尝试连接到: {}", url_string);
            match transport::connect_client(url_string.clone()).await {
                Ok(mut client_connection) => {
                    info!("[SatControlCenter] (连接任务) 连接成功建立。");
                    connection_attempt_successful = true;

                    // --- 连接成功后的处理 (原 handle_on_open 逻辑) ---
                    *is_connected_status_clone.write().await = true;
                    *last_pong_received_at_clone.write().await = Some(Utc::now());

                    // 发送连接成功事件
                    let event_payload = WsConnectionStatusEvent {
                        connected: true,
                        error_message: Some("成功连接到云端WebSocket服务".to_string()),
                        client_id: None, 
                    };
                    if let Err(e) = app_handle_clone.emit(WS_CONNECTION_STATUS_EVENT, &event_payload) {
                        error!(
                            "[SatControlCenter] (连接任务) 发送 \"已连接\" 事件 ({}) 失败: {}", 
                            WS_CONNECTION_STATUS_EVENT, e
                        );
                    }

                    // 存储发送通道
                    *ws_send_channel_clone.lock().await = Some(client_connection.ws_sender);

                    // 启动心跳任务
                    let hb_app_handle = app_handle_clone.clone();
                    let hb_ws_send = ws_send_channel_clone.clone();
                    let hb_last_pong = last_pong_received_at_clone.clone();
                    let hb_is_connected = is_connected_status_clone.clone();
                    let hb_cloud_id = cloud_assigned_client_id_clone.clone();
                    let hb_task = tokio::spawn(async move {
                        Self::run_heartbeat_loop(
                            hb_app_handle,
                            hb_ws_send,
                            hb_last_pong,
                            hb_is_connected,
                            hb_cloud_id,
                        ).await;
                    });
                    heartbeat_join_handle = Some(hb_task);

                    // --- 消息接收循环 ---
                    info!("[SatControlCenter] (连接任务) WebSocket 消息接收流 (Stream) 已启动。");
                    loop {
                        tokio::select! {
                            maybe_msg_result = transport::receive_message(&mut client_connection.ws_receiver) => {
                                match maybe_msg_result {
                                    Some(Ok(ws_msg)) => { // ws_msg is now correctly WsMessage
                                        Self::process_received_message(
                                            &app_handle_clone,
                                            ws_msg, 
                                            &cloud_assigned_client_id_clone,
                                            &last_pong_received_at_clone,
                                            &local_task_state_cache_clone,
                                        ).await;
                                    }
                                    Some(Err(e)) => {
                                        // Errors from transport::receive_message are WsUtilError
                                        match e {
                                            WsError::WebSocketProtocolError(ws_err) => {
                                                match ws_err {
                                                    tokio_tungstenite::tungstenite::Error::ConnectionClosed => {
                                                        warn!("[SatControlCenter] (连接任务) WebSocket 连接已由对方关闭 (via Tungstenite Error)。");
                                                        final_error_message = Some("WebSocket 连接已由对方关闭".to_string());
                                                        break;
                                                    }
                                                    tokio_tungstenite::tungstenite::Error::Protocol(protocol_err) => {
                                                        warn!("[SatControlCenter] (连接任务) WebSocket 协议错误 (例如收到Close帧): {:?}", protocol_err);
                                                        final_error_message = Some(format!("WebSocket 协议错误: {:?}", protocol_err));
                                                        break;
                                                    }
                                                    other_tungstenite_err => {
                                                        error!("[SatControlCenter] (连接任务) 接收消息时发生 Tungstenite 错误: {:?}", other_tungstenite_err);
                                                        final_error_message = Some(format!("接收消息时发生 Tungstenite 错误: {:?}", other_tungstenite_err));
                                                        break;
                                                    }
                                                }
                                            }
                                            WsError::DeserializationError(de_err) => {
                                                error!("[SatControlCenter] (连接任务) JSON (反)序列化错误: {:?}", de_err);
                                            }
                                            WsError::IoError(io_err) => {
                                                error!("[SatControlCenter] (连接任务) IO 错误: {:?}", io_err);
                                                final_error_message = Some(format!("IO 错误: {:?}", io_err));
                                                break;
                                            }
                                            _other_ws_util_err => { 
                                                error!("[SatControlCenter] (连接任务) 接收消息时发生未处理的 WsUtilError: {:?}", _other_ws_util_err);
                                                final_error_message = Some(format!("接收消息时发生未处理的库错误: {:?}", _other_ws_util_err));
                                                break; 
                                            }
                                        }
                                    }
                                    None => { // Stream ended, typically means connection closed by peer
                                        info!("[SatControlCenter] (连接任务) WebSocket 连接流已结束 (可能由对方关闭)。");
                                        final_error_message = final_error_message.or(Some("WebSocket 连接已由对方关闭".to_string()));
                                        break; 
                                    }
                                }
                            }
                            // Optional: Add a branch to select! for checking self.is_connected_status if manual disconnect is to break this loop
                            // _ = async { ... } => { ... break; }
                        }
                    }
                    // info!("[SatControlCenter] (连接任务) 已退出消息接收循环。"); // This log might be inside the loop in some logic branches
                }
                Err(e) => {
                    error!("[SatControlCenter] (连接任务) 连接到 WebSocket 服务器失败: {:?}", e);
                    final_error_message = Some(format!("连接到 WebSocket 服务器失败: {}", e.to_string()));
                    // connection_attempt_successful 保持 false
                }
            }

            // --- 连接结束后的清理与通知 ---
            info!("[SatControlCenter] (连接任务) 进入连接结束处理阶段。");

            // 停止心跳任务 (如果它正在运行)
            if let Some(hb_handle) = heartbeat_join_handle.take() {
                info!("[SatControlCenter] (连接任务) 正在停止心跳任务...");
                hb_handle.abort();
                match hb_handle.await {
                    Ok(_) => info!("[SatControlCenter] (连接任务) 心跳任务已成功中止或完成。"),
                    Err(e) if e.is_cancelled() => info!("[SatControlCenter] (连接任务) 心跳任务已被取消。"),
                    Err(e) => warn!("[SatControlCenter] (连接任务) 等待心跳任务结束时发生错误: {:?}", e),
                }
            }

            // 更新连接状态
            let was_connected = *is_connected_status_clone.read().await;
            *is_connected_status_clone.write().await = false;
            *ws_send_channel_clone.lock().await = None; // 清理发送通道
            *cloud_assigned_client_id_clone.write().await = None; // 清理 client_id
            *last_pong_received_at_clone.write().await = None; // 清理 pong 时间戳

            // 根据情况发送连接状态事件
            if connection_attempt_successful && was_connected { // 如果之前连接成功，现在断开了
                info!("[SatControlCenter] (连接任务) 连接已断开。发送断开事件。");
                let event_payload = WsConnectionStatusEvent {
                    connected: false,
                    error_message: final_error_message.or(Some("WebSocket 连接已断开".to_string())),
                    client_id: None, // 断开时 client_id 通常已无效或不需要
                };
                if let Err(e) = app_handle_clone.emit(WS_CONNECTION_STATUS_EVENT, &event_payload) {
                    error!(
                        "[SatControlCenter] (连接任务) 发送 \"已断开\" 事件 ({}) 失败: {}",
                        WS_CONNECTION_STATUS_EVENT, e
                    );
                }
            } else if !connection_attempt_successful { // 如果连接尝试一开始就失败了
                info!("[SatControlCenter] (连接任务) 连接尝试失败。发送失败事件。");
                let event_payload = WsConnectionStatusEvent {
                    connected: false,
                    error_message: final_error_message.or(Some("连接 WebSocket 失败，请检查网络或URL。".to_string())),
                    client_id: None,
                };
                if let Err(e) = app_handle_clone.emit(WS_CONNECTION_STATUS_EVENT, &event_payload) {
                    error!(
                        "[SatControlCenter] (连接任务) 发送 \"连接失败\" 事件 ({}) 失败: {}",
                        WS_CONNECTION_STATUS_EVENT, e
                    );
                }
            }
            // 如果 connection_attempt_successful 为 true 但 was_connected 为 false，
            // 这意味着 on_open 逻辑中已发送过连接成功事件，这里不需要重复发送失败或断开。

            info!("[SatControlCenter] (连接任务) 连接处理任务已结束。");
        });

        // 存储新的连接任务句柄
        *connection_task_handle_clone.lock().await = Some(connection_task);
        info!("[SatControlCenter] WebSocketClientService::connect: 新的连接处理任务已启动。");
        Ok(())
    }

    /// 处理从 WebSocket 服务器接收到的消息。
    ///
    /// 此函数负责解析消息类型，反序列化 Payload，并根据消息内容
    /// 执行相应的操作，如更新本地状态、发射 Tauri 事件通知前端等。
    ///
    /// # 参数
    /// * `app_handle`: Tauri 应用句柄。
    /// * `ws_msg`: 从服务器接收到的 `WsMessage`。
    /// * `cloud_assigned_client_id_state`: 用于存储云端分配的 client_id 的状态。
    /// * `last_pong_received_at_clone`: 用于更新最后收到 Pong 时间的状态。
    /// * `local_task_state_cache_clone`: 用于更新本地任务状态缓存的状态。
    async fn process_received_message(
        app_handle: &AppHandle,
        ws_msg: WsMessage,
        cloud_assigned_client_id_state: &Arc<RwLock<Option<Uuid>>>,
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
        local_task_state_cache_clone: &Arc<RwLock<Option<TaskDebugState>>>,
    ) {
        debug!(
            "[SatControlCenter] process_received_message: 收到消息类型 '{}'",
            ws_msg.message_type
        );

        match ws_msg.message_type.as_str() {
            ECHO_MESSAGE_TYPE => {
                match serde_json::from_str::<EchoPayload>(&ws_msg.payload) {
                    Ok(echo_payload) => {
                        info!(
                            "[SatControlCenter] 收到 Echo 回复，内容: '{}'",
                            echo_payload.content
                        );
                        let event_payload = EchoResponseEventPayload {
                            content: echo_payload.content,
                        };
                        if let Err(e) = app_handle.emit(ECHO_RESPONSE_EVENT, &event_payload) {
                            error!(
                                "[SatControlCenter] 发送 EchoResponseEvent ({}) 失败: {}",
                                ECHO_RESPONSE_EVENT, e
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            "[SatControlCenter] 反序列化 EchoPayload 失败: {}, 原始 payload: {}",
                            e, ws_msg.payload
                        );
                    }
                }
            }
            REGISTER_RESPONSE_MESSAGE_TYPE => {
                match serde_json::from_str::<RegisterResponsePayload>(&ws_msg.payload) {
                    Ok(payload) => {
                        info!(
                            "[SatControlCenter] 收到 RegisterResponse: success={}, client_id={:?}, group_id={:?}, role={:?}, msg='{}'",
                            payload.success,
                            payload.assigned_client_id,
                            payload.effective_group_id,
                            payload.effective_role,
                            payload.message.as_deref().unwrap_or("")
                        );
                        if payload.success {
                            *cloud_assigned_client_id_state.write().await = Some(payload.assigned_client_id);
                            // 更新连接状态事件，包含 client_id
                            let conn_event_payload = WsConnectionStatusEvent {
                                connected: true,
                                error_message: Some("客户端注册成功".to_string()),
                                client_id: Some(payload.assigned_client_id.to_string()),
                            };
                            if let Err(e) = app_handle.emit(WS_CONNECTION_STATUS_EVENT, &conn_event_payload) {
                                error!(
                                    "[SatControlCenter] 发送包含 client_id 的 WsConnectionStatusEvent ({}) 失败: {}",
                                    WS_CONNECTION_STATUS_EVENT, e
                                );
                            }
                        }
                        let reg_event_payload = WsRegistrationStatusEventPayload {
                            success: payload.success,
                            message: payload.message,
                            assigned_client_id: Some(payload.assigned_client_id.to_string()),
                            group_id: payload.effective_group_id,
                            role: payload.effective_role.map(|r| r.to_string()),
                            task_id: None,
                        };
                        if let Err(e) = app_handle.emit(WS_REGISTRATION_STATUS_EVENT, &reg_event_payload) {
                            error!(
                                "[SatControlCenter] 发送 WsRegistrationStatusEvent ({}) 失败: {}",
                                WS_REGISTRATION_STATUS_EVENT, e
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            "[SatControlCenter] 反序列化 RegisterResponsePayload 失败: {}, 原始 payload: {}",
                            e, ws_msg.payload
                        );
                    }
                }
            }
            PARTNER_STATUS_UPDATE_MESSAGE_TYPE => {
                match serde_json::from_str::<PartnerStatusPayload>(&ws_msg.payload) {
                    Ok(payload) => {
                        info!(
                            "[SatControlCenter] 收到 PartnerStatusUpdate: group='{}', partner_role={:?}, client_id={}, online={}",
                            payload.group_id, payload.partner_role, payload.partner_client_id, payload.is_online
                        );
                        let event_payload = WsPartnerStatusEventPayload {
                            partner_role: payload.partner_role.to_string(),
                            partner_client_id: Some(payload.partner_client_id.to_string()),
                            is_online: payload.is_online,
                            group_id: Some(payload.group_id),
                        };
                        if let Err(e) = app_handle.emit(WS_PARTNER_STATUS_EVENT, &event_payload) {
                            error!(
                                "[SatControlCenter] 发送 WsPartnerStatusEvent ({}) 失败: {}",
                                WS_PARTNER_STATUS_EVENT, e
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            "[SatControlCenter] 反序列化 PartnerStatusPayload 失败: {}, 原始 payload: {}",
                            e, ws_msg.payload
                        );
                    }
                }
            }
            TASK_STATE_UPDATE_MESSAGE_TYPE => {
                match serde_json::from_str::<TaskDebugState>(&ws_msg.payload) {
                    Ok(new_state) => {
                        info!(
                            "[中心端服务] (处理消息) 成功解析类型为 '{}' 的任务状态更新: 任务ID='{}', 最后更新者='{:?}', 时间戳={}",
                            TASK_STATE_UPDATE_MESSAGE_TYPE,
                            new_state.task_id,
                            new_state.last_updated_by_role,
                            new_state.last_update_timestamp
                        );
                        info!("[中心端服务] (处理消息) 收到的完整 TaskDebugState: {:?}", new_state);

                        *local_task_state_cache_clone.write().await = Some(new_state.clone());
                        
                        let event_payload = LocalTaskStateUpdatedEventPayload {
                            new_state: new_state,
                        };
                        if let Err(e) = app_handle.emit(LOCAL_TASK_STATE_UPDATED_EVENT, &event_payload) {
                            error!(
                                "[SatControlCenter] 发送 LocalTaskStateUpdatedEvent ({}) 失败: {}",
                                LOCAL_TASK_STATE_UPDATED_EVENT, e
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            "[SatControlCenter] 反序列化 TaskDebugState (for TaskStateUpdate) 失败: {}, 原始 payload: {}",
                            e, ws_msg.payload
                        );
                    }
                }
            }
            PONG_MESSAGE_TYPE => {
                debug!("[SatControlCenter] 收到 Pong 消息。");
                *last_pong_received_at_clone.write().await = Some(Utc::now());
            }
            ERROR_RESPONSE_MESSAGE_TYPE => {
                 match serde_json::from_str::<ErrorResponsePayload>(&ws_msg.payload) {
                    Ok(payload) => {
                        warn!(
                            "[SatControlCenter] 收到来自云端的错误响应: message='{}', original_request_type='{:?}'",
                            payload.error, payload.original_message_type
                        );
                        // TODO: 通过 Tauri 事件将此错误信息传递给前端，以便UI可以显示
                        // 例如，可以定义一个新的 WsServerErrorEvent
                        let error_event_payload = WsServerErrorEventPayload {
                            error_message: payload.error,
                            original_message_type: payload.original_message_type,
                        };
                        if let Err(e) = app_handle.emit(WS_SERVER_ERROR_EVENT, &error_event_payload) {
                            error!(
                                "[SatControlCenter] 发送 WsServerErrorEvent ({}) 失败: {}",
                                WS_SERVER_ERROR_EVENT, e
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            "[SatControlCenter] 反序列化 ErrorResponsePayload 失败: {}, 原始 payload: {}",
                            e, ws_msg.payload
                        );
                    }
                }
            }
            unknown_type => {
                warn!(
                    "[SatControlCenter] 收到未知类型的 WebSocket 消息: '{}'. 忽略此消息. Payload: {}",
                    unknown_type, ws_msg.payload
                );
            }
        }
    }

    /// 心跳循环。
    ///
    /// 此异步函数在一个循环中运行，定期发送 Ping 消息到服务器，并检查是否超时未收到 Pong 回复。
    /// 当 `is_connected_status_clone` 变为 `false` 时，或者当任务被取消时，循环会终止。
    ///
    /// # Arguments
    /// * `app_handle`: Tauri 应用句柄 (当前未使用，但保留以备将来可能需要发送事件)。
    /// * `ws_send_channel_clone`: WebSocket 发送通道。
    /// * `last_pong_received_at_clone`: 最后收到 Pong 的时间戳。
    /// * `is_connected_status_clone`: 当前连接状态。
    /// * `_cloud_assigned_client_id_clone`: 客户端ID (当前未使用)。
    async fn run_heartbeat_loop(
        _app_handle: AppHandle, // 当前未使用，加下划线表示
        ws_send_channel_clone: Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        last_pong_received_at_clone: Arc<RwLock<Option<DateTime<Utc>>>>,
        is_connected_status_clone: Arc<RwLock<bool>>,
        _cloud_assigned_client_id_clone: Arc<RwLock<Option<Uuid>>>, // 参数保留，但当前未使用，加下划线
    ) {
        info!("[SatControlCenter] (心跳任务) 心跳监控已启动，间隔 {} 秒，Pong超时 {} 秒。", HEARTBEAT_INTERVAL_SECONDS, PONG_TIMEOUT_SECONDS);
        let heartbeat_interval = Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS);
        let pong_timeout = Duration::from_secs(PONG_TIMEOUT_SECONDS);

        loop {
            // 等待心跳间隔或任务被取消
            if tokio::time::timeout(heartbeat_interval, tokio::signal::ctrl_c()) // 使用 ctrl_c 作为可取消的 future
                .await.is_ok() { // 如果 ctrl_c 信号被接收 (或超时，但这里主要关心取消点)
                info!("[SatControlCenter] (心跳任务) 收到取消信号或超时，正在终止心跳任务...");
                break;
            }

            // 检查连接状态
            if !*is_connected_status_clone.read().await {
                info!("[SatControlCenter] (心跳任务) 检测到连接已断开，正在终止心跳任务...");
                break;
            }

            // 检查 Pong 超时
            let last_pong_opt = *last_pong_received_at_clone.read().await;
            if let Some(last_pong_time) = last_pong_opt {
                // Convert std::time::Duration to chrono::Duration for comparison
                let timeout_duration_chrono = match chrono::Duration::from_std(pong_timeout + heartbeat_interval) {
                    Ok(cd) => cd,
                    Err(e) => {
                        error!("[SatControlCenter] (心跳任务) 无法将 std::time::Duration 转换为 chrono::Duration: {:?}", e);
                        // Fallback or break, here we break to be safe
                        break;
                    }
                };
                if Utc::now().signed_duration_since(last_pong_time) > timeout_duration_chrono {
                    warn!(
                        "[SatControlCenter] (心跳任务) Pong 响应超时 (超过 {} 秒未收到 Pong)。可能连接已死。",
                        (pong_timeout + heartbeat_interval).as_secs()
                    );
                    // 注意：这里仅记录警告。实际的连接断开和清理应由主连接任务的 `on_close` 或 `on_error` 处理，
                    // 或者由更高级的重连策略管理。心跳任务本身不直接关闭连接。
                    // 如果需要主动断开，可以考虑发送一个信号给主连接任务或调用 disconnect。
                }
            } else {
                // 如果从未收到过 Pong (例如连接刚建立或有问题)，也记录一下
                warn!("[SatControlCenter] (心跳任务) 从未收到过 Pong 或 Pong 时间戳已被重置。");
            }

            // 发送 Ping 消息
            debug!("[SatControlCenter] (心跳任务) 发送 Ping 消息...");
            let ping_payload = PingPayload {}; // Empty struct for PingPayload
            match serde_json::to_string(&ping_payload) { // Serialize PingPayload
                Ok(json_payload) => {
                    // Manually construct WsMessage if not using WsMessage::new helper which handles id and timestamp
                    let ws_message = WsMessage {
                        message_id: Uuid::new_v4().to_string(), // Add message_id
                        timestamp: Utc::now().timestamp_millis(), // Add timestamp
                        message_type: PING_MESSAGE_TYPE.to_string(),
                        payload: json_payload,
                    };
                    let mut sender_guard = ws_send_channel_clone.lock().await;
                    if let Some(ref mut sender) = *sender_guard {
                        match serde_json::to_string(&ws_message) { // Serialize WsMessage to JSON string
                            Ok(msg_json_str) => {
                                if let Err(e) = sender.send(TungsteniteMessage::Text(msg_json_str)).await {
                                    error!("[SatControlCenter] (心跳任务) 发送 Ping 消息失败: {:?}", e);
                                    // 发送失败可能意味着连接已断开，心跳循环将在下次检查 is_connected 时终止
                                } else {
                                    debug!("[SatControlCenter] (心跳任务) Ping 消息已发送。");
                                }
                            },
                            Err(e) => {
                                error!("[SatControlCenter] (心跳任务) 序列化 WsMessage (Ping) 失败: {:?}", e);
                            }
                        }
                    } else {
                        warn!("[SatControlCenter] (心跳任务) 尝试发送 Ping 但 WebSocket 发送通道不存在 (可能已断开)。");
                        // 心跳循环将在下次检查 is_connected 时终止
                    }
                }
                Err(e) => {
                    error!("[SatControlCenter] (心跳任务) 序列化 PingPayload 失败: {:?}", e);
                }
            }
        }
        info!("[SatControlCenter] (心跳任务) 心跳监控已停止。");
    }

    /// 主动断开当前的 WebSocket 连接。
    ///
    /// 此方法会尝试优雅地关闭 WebSocket 连接，并中止相关的处理任务 (连接任务和心跳任务)。
    ///
    /// # 返回
    /// * `Result<(), String>`: 如果操作成功启动，则返回 `Ok(())`。
    ///   如果发送通道不存在或关闭连接时发生错误，则返回 `Err(String)`。
    pub async fn disconnect(&self) -> Result<(), String> {
        info!("[SatControlCenter] WebSocketClientService::disconnect 调用。");

        // 标记不再连接，这将有助于心跳任务等自行终止
        *self.is_connected_status.write().await = false;

        // 停止并清理心跳任务句柄
        let mut hb_task_guard = self.heartbeat_task_handle.lock().await;
        if let Some(hb_handle) = hb_task_guard.take() {
            info!("[SatControlCenter] 正在中止心跳任务...");
            hb_handle.abort();
            match hb_handle.await {
                Ok(_) => info!("[SatControlCenter] 心跳任务已成功中止或完成。"),
                Err(e) if e.is_cancelled() => info!("[SatControlCenter] 心跳任务已被取消。"),
                Err(e) => warn!("[SatControlCenter] 等待心跳任务结束时发生错误: {:?}", e),
            }
        }
        drop(hb_task_guard);

        // 尝试优雅关闭 WebSocket 发送端
        let mut sender_guard = self.ws_send_channel.lock().await;
        if let Some(ref mut sender) = *sender_guard {
            info!("[SatControlCenter] 正在尝试关闭 WebSocket 发送通道...");
            if let Err(e) = sender.close().await {
                warn!("[SatControlCenter] 关闭 WebSocket 发送通道时发生错误: {:?}", e);
                // 即使关闭失败，也继续清理
            } else {
                info!("[SatControlCenter] WebSocket 发送通道已成功请求关闭。");
            }
        }
        *sender_guard = None; // 清理发送通道句柄
        drop(sender_guard);

        // 停止并清理主连接任务句柄
        let mut conn_task_guard = self.connection_task_handle.lock().await;
        if let Some(conn_handle) = conn_task_guard.take() {
            info!("[SatControlCenter] 正在中止主连接任务...");
            conn_handle.abort(); // 请求取消主连接任务
            match conn_handle.await {
                Ok(_) => info!("[SatControlCenter] 主连接任务已成功中止或完成。"),
                Err(e) if e.is_cancelled() => info!("[SatControlCenter] 主连接任务已被取消。"),
                Err(e) => warn!("[SatControlCenter] 等待主连接任务结束时发生错误: {:?}", e),
            }
        }
        drop(conn_task_guard);
        
        // 清理其他状态
        *self.cloud_assigned_client_id.write().await = None;
        *self.last_pong_received_at.write().await = None;

        // 发送断开连接事件到前端 (如果之前是连接状态)
        // 注意：主连接任务的结束逻辑中通常也会发送此事件，这里可能重复，但为了确保，可以发送。
        // 或者依赖主连接任务的清理逻辑。
        info!("[SatControlCenter] 发送显式断开连接状态事件。");
        let event_payload = WsConnectionStatusEvent {
            connected: false,
            error_message: Some("用户请求断开连接".to_string()),
            client_id: None,
        };
        if let Err(e) = self.app_handle.emit(WS_CONNECTION_STATUS_EVENT, &event_payload) {
            error!(
                "[SatControlCenter] 发送显式断开事件 ({}) 失败: {}",
                WS_CONNECTION_STATUS_EVENT, e
            );
        }

        info!("[SatControlCenter] WebSocketClientService::disconnect 完成。");
        Ok(())
    }

    /// 检查当前 WebSocket 是否已连接。
    ///
    /// # 返回
    /// * `bool`: 如果已连接则为 `true`，否则为 `false`。
    pub async fn is_connected(&self) -> bool {
        *self.is_connected_status.read().await
    }

    /// 获取当前云端分配的客户端 ID (如果已连接并注册)。
    ///
    /// # 返回
    /// * `Option<Uuid>`: 如果已分配，则返回客户端 ID，否则返回 `None`。
    pub async fn get_cloud_assigned_client_id(&self) -> Option<Uuid> {
        *self.cloud_assigned_client_id.read().await
    }

    /// 发送一个 `WsMessage` 到已连接的 WebSocket 服务器。
    ///
    /// # 参数
    /// * `message`: 要发送的 `WsMessage` 实例。
    ///
    /// # 返回
    /// * `Result<(), String>`: 如果消息成功放入发送队列，则返回 `Ok(())`。
    ///   如果未连接或发送失败，则返回 `Err(String)`。
    pub async fn send_ws_message(&self, message: WsMessage) -> Result<(), String> {
        if !self.is_connected().await {
            let err_msg = "[SatControlCenter] 发送 WebSocket 消息失败：未连接到服务器。".to_string();
            warn!("{}", err_msg);
            return Err(err_msg);
        }

        let mut sender_guard = self.ws_send_channel.lock().await;
        if let Some(ref mut sender) = *sender_guard {
            let message_type_for_log = message.message_type.clone(); // 用于日志
            let payload_summary_for_log = truncate_string(&message.payload, 100); // 日志中 Payload 摘要
            match serde_json::to_string(&message) { // Serialize WsMessage to JSON string
                Ok(msg_json_str) => {
                    match sender.send(TungsteniteMessage::Text(msg_json_str)).await {
                        Ok(_) => {
                            debug!(
                                "[SatControlCenter] WebSocket 消息已发送: 类型='{}', Payload摘要='{}'",
                                message_type_for_log,
                                payload_summary_for_log
                            );
                            Ok(())
                        }
                        Err(e) => {
                            let err_msg = format!(
                                "[SatControlCenter] 发送 WebSocket 消息 (类型='{}') 失败: {:?}",
                                message_type_for_log, e
                            );
                            error!("{}", err_msg);
                            Err(err_msg)
                        }
                    }
                }
                Err(e) => {
                    let err_msg = format!(
                        "[SatControlCenter] 序列化 WebSocket 消息 (类型='{}') 失败: {:?}",
                        message_type_for_log, e
                    );
                    error!("{}", err_msg);
                    Err(err_msg)
                }
            }
        } else {
            let err_msg = "[SatControlCenter] 发送 WebSocket 消息失败：发送通道不可用 (可能已断开)。".to_string();
            warn!("{}", err_msg);
            Err(err_msg)
        }
    }

    /// 获取本地缓存的最新 `TaskDebugState`。
    ///
    /// 此方法是同步的，因为它只是读取 `RwLock` 保护的数据。
    ///
    /// # 返回
    /// * `Option<TaskDebugState>`: 如果缓存中有状态，则返回其克隆，否则返回 `None`。
    pub async fn get_cached_task_state(&self) -> Option<TaskDebugState> {
        self.local_task_state_cache.read().await.clone()
    }

    /// 发送 Echo 消息到 WebSocket 服务器。
    pub async fn send_echo_message(&self, payload: EchoPayload) -> Result<(), String> {
        info!(
            "[SatControlCenter] WebSocketClientService::send_echo_message 调用，内容: '{}'", // 日志前缀修改
            payload.content
        );
        if !self.is_connected().await {
            let err_msg = "WebSocket 未连接，无法发送 Echo 消息。".to_string();
            error!("[SatControlCenter] {}", err_msg); // 日志前缀修改
            return Err(err_msg);
        }

        match WsMessage::new(ECHO_MESSAGE_TYPE.to_string(), &payload) {
            Ok(ws_message) => self.send_ws_message(ws_message).await,
            Err(e) => {
                let err_msg = format!("创建 Echo WsMessage 失败: {:?}", e);
                error!("[SatControlCenter] {}", err_msg); // 日志前缀修改
                Err(err_msg)
            }
        }
    }

    /// 发送特定类型的业务消息到 WebSocket 服务器。
    /// 这是一个通用方法，用于封装构建 WsMessage 并发送的逻辑。
    pub async fn send_specific_message<T>(
        &self, 
        message_type: &str, 
        payload_obj: &T
    ) -> Result<(), String> 
    where 
        T: serde::Serialize + Send + Sync + std::fmt::Debug
    {
        info!(
            "[SatControlCenter] WebSocketClientService::send_specific_message 调用，类型: '{}', Payload: {:?}", // 日志前缀修改
            message_type, payload_obj
        );
        if !self.is_connected().await {
            let err_msg = format!("WebSocket 未连接，无法发送类型为 '{}' 的消息。", message_type);
            error!("[SatControlCenter] {}", err_msg); // 日志前缀修改
            return Err(err_msg);
        }

        match WsMessage::new(message_type.to_string(), payload_obj) {
            Ok(ws_message) => {
                debug!("[SatControlCenter] 准备发送构建好的WsMessage: ID={}, Type={}, Timestamp={}", ws_message.message_id, ws_message.message_type, ws_message.timestamp); // 日志前缀修改
                self.send_ws_message(ws_message).await
            }
            Err(e) => {
                let err_msg = format!("创建类型为 '{}' 的 WsMessage 失败: {:?}", message_type, e);
                error!("[SatControlCenter] {}", err_msg); // 日志前缀修改
                Err(err_msg)
            }
        }
    }
}

/// 辅助函数，用于截断字符串以便在日志中显示摘要。
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}... (truncated)", &s[..max_len.saturating_sub(20)]) // 留出空间给 ... (truncated)
    } else {
        s.to_string()
    }
}