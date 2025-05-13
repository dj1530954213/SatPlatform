// SatControlCenter/src-tauri/src/ws_client/service.rs

//! `SatControlCenter` (控制中心) 应用的 WebSocket 客户端服务模块。
//!
//! 该模块负责管理与云端 `SatCloudService` 的 WebSocket 连接，
//! 包括连接建立、断开、消息收发、状态同步以及心跳维持。

use anyhow::Result;
use log::{debug, error, info, warn};
use rust_websocket_utils::client::transport;
use rust_websocket_utils::client::transport::ClientWsStream;
// use rust_websocket_utils::message::WsMessage;
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
use rust_websocket_utils::message::WsMessage;

// 注意：这里的事件导入需要确保与 SatControlCenter/src-tauri/src/event.rs 中的定义一致
use crate::event::{
    WS_CONNECTION_STATUS_EVENT, WsConnectionStatusEvent, ECHO_RESPONSE_EVENT, EchoResponseEventPayload,
    WS_REGISTRATION_STATUS_EVENT, WsRegistrationStatusEventPayload,
    WS_PARTNER_STATUS_EVENT, WsPartnerStatusEventPayload,
    LOCAL_TASK_STATE_UPDATED_EVENT, LocalTaskStateUpdatedEventPayload,
    // WS_SERVER_ERROR_EVENT, WsServerErrorEventPayload, // 如果需要处理服务器错误，需要定义这个事件
};
// 注意：这里的 common_models 导入应该不需要更改
use common_models::{
    self,
    ws_payloads::{
        PING_MESSAGE_TYPE, PONG_MESSAGE_TYPE, PingPayload,
        REGISTER_RESPONSE_MESSAGE_TYPE, RegisterResponsePayload,
        PARTNER_STATUS_UPDATE_MESSAGE_TYPE, PartnerStatusPayload,
        TASK_STATE_UPDATE_MESSAGE_TYPE,
        ECHO_MESSAGE_TYPE, // Assuming EchoPayload is constructed elsewhere
        // ERROR_RESPONSE_MESSAGE_TYPE, ErrorResponsePayload, // Removed unused imports
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
        // **日志标识符已更改**
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
        // **日志标识符已更改**
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
            // **日志标识符已更改**
            info!("[SatControlCenter] 检测到之前的连接任务正在运行，正在尝试取消...");
            handle.abort(); // 请求取消任务
            match handle.await { // 等待任务结束（即使是被取消的）
                // **日志标识符已更改**
                Ok(_) => info!("[SatControlCenter] 之前的连接任务已成功取消或完成。"),
                // **日志标识符已更改**
                Err(e) if e.is_cancelled() => info!("[SatControlCenter] 之前的连接任务已被取消。"),
                // **日志标识符已更改**
                Err(e) => warn!("[SatControlCenter] 等待之前的连接任务结束时发生错误: {:?}", e),
            }
            // **日志标识符已更改**
            info!("[SatControlCenter] 已清理之前的连接任务句柄。");
            // 清理相关状态
            *is_connected_status_clone.write().await = false;
            *ws_send_channel_clone.lock().await = None;
            *cloud_assigned_client_id_clone.write().await = None;
            // 心跳任务句柄也应停止
            let mut hb_task_guard = heartbeat_task_handle_clone.lock().await;
            if let Some(hb_handle) = hb_task_guard.take() {
                 hb_handle.abort();
                 // **日志标识符已更改**
                 info!("[SatControlCenter] 已请求取消相关的心跳任务。");
                 // 不必 await 心跳任务的结束，因为它应该很快响应 abort
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
            // **日志标识符已更改**
            info!("[SatControlCenter] (连接任务) 开始尝试连接到: {}", url_string);
            match transport::connect_client(url_string.clone()).await {
                Ok(mut client_connection) => {
                    // **日志标识符已更改**
                    info!("[SatControlCenter] (连接任务) 连接成功建立。");
                    connection_attempt_successful = true;

                    // --- 连接成功后的处理 (原 handle_on_open 逻辑) ---
                    *is_connected_status_clone.write().await = true;
                    // cloud_assigned_client_id 在注册响应后更新
                    *last_pong_received_at_clone.write().await = Some(Utc::now());

                    // 发送连接成功事件
                    // **确保 WsConnectionStatusEvent 在 SatControlCenter 的 event.rs 中定义正确**
        let event_payload = WsConnectionStatusEvent {
            connected: true,
                        error_message: Some("成功连接到云端WebSocket服务".to_string()),
                        client_id: None, // 在 RegisterResponse 后才确定
                    };
                    // **确保 WS_CONNECTION_STATUS_EVENT 常量在 SatControlCenter 的 event.rs 中定义正确**
                    if let Err(e) = app_handle_clone.emit_to("main", WS_CONNECTION_STATUS_EVENT, &event_payload) {
                        // **日志标识符已更改**
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
                        // 调用静态方法运行心跳循环
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
                    loop {
                        tokio::select! {
                            // 接收消息
                            // 使用 rust_websocket_utils 提供的辅助函数接收和解析消息
                            maybe_msg_result = transport::receive_message(&mut client_connection.ws_receiver) => {
                                match maybe_msg_result {
                                    Some(Ok(ws_msg)) => {
                                        // --- 处理收到的消息 (原 handle_on_message 逻辑) ---
                                         // 调用静态方法处理消息
                                         Self::process_received_message(
                                             &app_handle_clone,
                                             ws_msg,
                                             &cloud_assigned_client_id_clone, // 注意传递的是克隆的 Arc 引用
                                             &last_pong_received_at_clone,
                                             &local_task_state_cache_clone,
                                         ).await;
                                    }
                                    Some(Err(e)) => {
                                        // --- 处理接收或解析错误 (原 handle_on_error 部分逻辑) ---
                                        // **日志标识符已更改**
                                        error!("[SatControlCenter] (连接任务) 消息接收或解析时发生错误: {}", e);
                                        final_error_message = Some(format!("接收消息时出错: {}", e));
                                        break; // 发生错误，退出接收循环
                                    }
                                    None => {
                                        // --- 连接已关闭 (原 handle_on_close 逻辑) ---
                                        // **日志标识符已更改**
                                        info!("[SatControlCenter] (连接任务) WebSocket 连接已由对端关闭。");
                                        final_error_message = Some("连接已由对端关闭".to_string());
                                        break; // 连接关闭，退出接收循环
                                    }
                                }
                            }
                            // 检查外部断开信号 (通过 is_connected_status 标志)
                            _ = async {
                                 loop {
                                     // 短暂休眠避免忙等待
                                     tokio::time::sleep(Duration::from_millis(500)).await;
                                     // 检查连接状态标志
                                     if !*is_connected_status_clone.read().await {
                                         break; // 如果状态变为 false，退出等待
                                     }
                                 }
                            } => {
                                 // **日志标识符已更改**
                                 info!("[SatControlCenter] (连接任务) 检测到连接状态变为 false，主动退出任务。");
                                 // 设置错误消息，除非已有更具体的错误
                                 final_error_message = final_error_message.or_else(|| Some("连接被主动断开".to_string()));
                                 break; // 退出 select 循环
                            }
                        }
                    }
                }
                Err(e) => {
                    // --- 处理连接错误 (原 handle_on_error 部分逻辑) ---
                    // **日志标识符已更改**
                    error!("[SatControlCenter] (连接任务) 连接到 {} 失败: {}", url_string, e);
                    final_error_message = Some(format!("连接失败: {}", e));
                    // 连接失败，connection_attempt_successful 保持 false
                }
            }

            // --- 清理阶段 (无论连接成功与否，或循环退出原因) ---
            // **日志标识符已更改**
            info!("[SatControlCenter] (连接任务) 进入清理阶段...");

            // 停止心跳任务 (如果已启动)
            if let Some(hb_handle) = heartbeat_join_handle {
                // **日志标识符已更改**
                info!("[SatControlCenter] (连接任务) 正在停止心跳任务...");
                hb_handle.abort();
                // 可以选择 await，但通常 abort 即可
                 // 清理主服务中存储的心跳任务句柄
                 let mut hb_guard = heartbeat_task_handle_clone.lock().await;
                 if hb_guard.is_some() { *hb_guard = None; } 
            }

            // 更新最终连接状态
            let was_connected = *is_connected_status_clone.read().await; // 记录断开前的状态
            *is_connected_status_clone.write().await = false;
            *cloud_assigned_client_id_clone.write().await = None;
            *ws_send_channel_clone.lock().await = None;
            *last_pong_received_at_clone.write().await = None;
            // 注意：local_task_state_cache_clone 不需要在此处清除，可以保持最后的状态，直到下次成功注册

            // 发送最终的连接状态事件 (只有在从未成功连接过，或者之前是连接状态时才发送)
            if !connection_attempt_successful || was_connected {
                // **确保 WsConnectionStatusEvent 定义正确**
                let event_payload = WsConnectionStatusEvent {
                    connected: false,
                    // 提供最终的错误信息，或者默认的关闭信息
                    error_message: final_error_message.or_else(|| Some("连接已关闭".to_string())), 
                    client_id: None, // 断开时通常不需要发送 client_id
                };
                // **确保 WS_CONNECTION_STATUS_EVENT 常量定义正确**
                if let Err(e) = app_handle_clone.emit_to("main", WS_CONNECTION_STATUS_EVENT, &event_payload) {
                    // **日志标识符已更改**
                    error!(
                        "[SatControlCenter] (连接任务-清理) 发送 \"连接关闭/失败\" 事件 ({}) 失败: {}",
                        WS_CONNECTION_STATUS_EVENT, e
                    );
                }
            }

            // 从主服务中移除当前连接任务的任务句柄 (表示任务已结束)
            let mut task_guard = connection_task_handle_clone.lock().await;
            // 可以通过比较 JoinHandle 的 id 来确保移除的是正确的任务句柄，但 JoinHandle 不直接暴露 ID。
            // 假设我们总是先取消旧任务再启动新任务，所以这里的 None 检查足够。
             if task_guard.is_some() {
                 *task_guard = None; // 移除句柄
                 // **日志标识符已更改**
                 info!("[SatControlCenter] (连接任务) 已清理自身在主服务中的任务句柄。");
             } else {
                 // **日志标识符已更改**
                 // 这可能发生在任务完成前，新的 connect 调用覆盖了句柄
                 info!("[SatControlCenter] (连接任务) 自身任务句柄已不在主服务中，可能已被新连接任务覆盖。");
             }


            // **日志标识符已更改**
            info!("[SatControlCenter] (连接任务) 已完成。");
        });

        // 存储新启动的连接任务的句柄
        let mut task_guard = self.connection_task_handle.lock().await;
        *task_guard = Some(connection_task);
        // **日志标识符已更改**
        info!("[SatControlCenter] WebSocket 连接任务已成功启动并存储句柄。");

        Ok(()) // 连接启动过程本身成功
    }

    /// 辅助函数：处理接收到的单个 WebSocket 消息。
    /// 这是一个静态方法，因为它不直接修改 `self` 的状态，而是通过传入的 Arc 引用来操作。
    async fn process_received_message(
        app_handle: &AppHandle,
        ws_msg: WsMessage,
        cloud_assigned_client_id_state: &Arc<RwLock<Option<Uuid>>>,
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
        local_task_state_cache_clone: &Arc<RwLock<Option<TaskDebugState>>>,
    ) {
        // **日志标识符已更改**
        // 增加 Payload 摘要日志
        info!(
            "[SatControlCenter] (处理消息) 类型='{}', 消息ID='{}', 时间戳='{}', Payload摘要='{}'", 
            ws_msg.message_type, ws_msg.message_id, ws_msg.timestamp, ws_msg.payload.chars().take(100).collect::<String>()
        ); 

        match ws_msg.message_type.as_str() {
            // 处理 Echo 消息（通常是服务器对客户端 Ping 的响应，或测试消息）
            // 根据 common_models 定义的消息类型常量进行匹配
            common_models::ws_payloads::ECHO_MESSAGE_TYPE => {
                 // 尝试将 payload 解析为 EchoPayload
                 match serde_json::from_str::<common_models::ws_payloads::EchoPayload>(&ws_msg.payload) {
                    Ok(echo_payload) => {
                        // **日志标识符已更改**
                        info!("[SatControlCenter] (处理消息) 成功解析 EchoPayload: {:?}", echo_payload);
                        // 构建要发送给前端的事件 Payload
                        // **确保 EchoResponseEventPayload 定义正确**
                        let response_event_payload = EchoResponseEventPayload { content: echo_payload.content };
                        // **确保 ECHO_RESPONSE_EVENT 常量定义正确**
                        if let Err(e) = app_handle.emit_to("main", ECHO_RESPONSE_EVENT, &response_event_payload) {
                             // **日志标识符已更改**
                             error!("[SatControlCenter] (处理消息) 发送 Echo响应 ({}) 事件失败: {}", ECHO_RESPONSE_EVENT, e);
                        }
                    }
                    Err(e) => {
                        // **日志标识符已更改**
                        error!("[SatControlCenter] (处理消息) 解析 EchoPayload 失败: {}. 原始Payload: {}", e, ws_msg.payload);
                    }
                }
            }
            // 处理 Pong 消息 (心跳响应)
            PONG_MESSAGE_TYPE => {
                // **日志标识符已更改**
                info!("[SatControlCenter] (处理消息) 收到 Pong 消息。");
                // 更新最后收到 Pong 的时间戳
                *last_pong_received_at_clone.write().await = Some(Utc::now());
            }
            // 处理注册响应消息
            REGISTER_RESPONSE_MESSAGE_TYPE => {
                // 尝试解析为 RegisterResponsePayload
                match serde_json::from_str::<RegisterResponsePayload>(&ws_msg.payload) {
                    Ok(payload) => {
                        // **日志标识符已更改**
                        info!("[SatControlCenter] (处理消息) 收到并解析 RegisterResponsePayload: {:?}", payload);
                        
                        // 如果注册成功，存储云端分配的客户端 ID
                        let mut assigned_id_str: Option<String> = None;
                        if payload.success {
                            let assigned_id = payload.assigned_client_id; // 是 Uuid 类型
                            *cloud_assigned_client_id_state.write().await = Some(assigned_id);
                            assigned_id_str = Some(assigned_id.to_string()); // 转换为字符串以匹配事件 Payload
                            // **日志标识符已更改**
                            info!("[SatControlCenter] (处理消息) 客户端注册成功，云端分配的客户端ID: {}", assigned_id);
                        } else {
                            // **日志标识符已更改**
                            warn!("[SatControlCenter] (处理消息) 客户端注册失败。原因: {:?}", payload.message);
                            // 注册失败，清除可能存在的旧 ID
                            *cloud_assigned_client_id_state.write().await = None;
                        }

                        // 构建要发送给前端的事件 Payload
                        // **确保 WsRegistrationStatusEventPayload 定义正确**
                        let event_payload = WsRegistrationStatusEventPayload {
                            success: payload.success,
                            message: payload.message.clone(),
                            group_id: payload.effective_group_id.clone(), // common_models::RegisterResponsePayload.effective_group_id 是 Option<String>
                            task_id: None, // 当前逻辑中 task_id 不通过 RegisterResponsePayload 传递，保持 None
                            assigned_client_id: assigned_id_str, // 使用转换后的 Option<String>
                        };
                        
                        // 发送注册状态事件到前端
                        // **确保 WS_REGISTRATION_STATUS_EVENT 常量定义正确**
                        if let Err(e) = app_handle.emit_to("main", WS_REGISTRATION_STATUS_EVENT, &event_payload) {
                            // **日志标识符已更改**
                            error!("[SatControlCenter] (处理消息) 发送 WsRegistrationStatusEventPayload ({}) 事件失败: {}", WS_REGISTRATION_STATUS_EVENT, e);
                        }
                    }
                    Err(e) => {
                        // **日志标识符已更改**
                        error!("[SatControlCenter] (处理消息) 解析 RegisterResponsePayload 失败: {}. 原始Payload: {}", e, ws_msg.payload);
                    }
                }
            }
            // 处理伙伴状态更新消息
            PARTNER_STATUS_UPDATE_MESSAGE_TYPE => {
                // 尝试解析为 PartnerStatusPayload
                match serde_json::from_str::<PartnerStatusPayload>(&ws_msg.payload) {
                    Ok(payload) => {
                        // **日志标识符已更改**
                        info!("[SatControlCenter] (处理消息) 收到并解析 PartnerStatusPayload: {:?}", payload);
                        // 构建事件 Payload 发送给前端
                        // **确保 WsPartnerStatusEventPayload 定义正确**
                        let event_payload = WsPartnerStatusEventPayload {
                            // 注意：payload.partner_role 是 ClientRole 枚举，需要转换为字符串
                            partner_role: payload.partner_role.to_string(), 
                            is_online: payload.is_online,
                        };
                        // **确保 WS_PARTNER_STATUS_EVENT 常量定义正确**
                        if let Err(e) = app_handle.emit_to("main", WS_PARTNER_STATUS_EVENT, &event_payload) {
                            // **日志标识符已更改**
                            error!("[SatControlCenter] (处理消息) 发送 WsPartnerStatusEventPayload ({}) 事件失败: {}", WS_PARTNER_STATUS_EVENT, e);
                        }
                    }
                    Err(e) => {
                        // **日志标识符已更改**
                        error!("[SatControlCenter] (处理消息) 解析 PartnerStatusPayload 失败: {}. 原始Payload: {}", e, ws_msg.payload);
                    }
                }
            }
            // 处理任务状态更新消息 (来自云端的权威状态)
            TASK_STATE_UPDATE_MESSAGE_TYPE => {
                // 尝试解析为 TaskDebugState
                match serde_json::from_str::<TaskDebugState>(&ws_msg.payload) {
                    Ok(new_state) => {
                        // **日志标识符已更改**
                        info!("[SatControlCenter] (处理消息) 收到并解析 TaskDebugState (任务ID: {}), 最后更新者: {:?}, 更新时间戳: {}", 
                            new_state.task_id, new_state.last_updated_by_role, new_state.last_update_timestamp
                        );
                        // 更新本地缓存的任务状态
                        *local_task_state_cache_clone.write().await = Some(new_state.clone());
                        
                        // 构建事件 Payload 发送给前端
                        // **确保 LocalTaskStateUpdatedEventPayload 定义正确**
                        // 不再需要将 TaskDebugState 序列化为 serde_json::Value
                        let event_payload = LocalTaskStateUpdatedEventPayload { new_state: new_state.clone() }; // 直接使用 new_state (TaskDebugState)
                        // **确保 LOCAL_TASK_STATE_UPDATED_EVENT 常量定义正确**
                        if let Err(e) = app_handle.emit_to("main", LOCAL_TASK_STATE_UPDATED_EVENT, &event_payload) {
                            // **日志标识符已更改**
                            error!("[SatControlCenter] (处理消息) 发送 LocalTaskStateUpdatedEventPayload ({}) 事件失败: {}", LOCAL_TASK_STATE_UPDATED_EVENT, e);
                        }
                    }
                    Err(e) => {
                        // **日志标识符已更改**
                        error!("[SatControlCenter] (处理消息) 解析 TaskDebugState 失败: {}. 原始Payload: {}", e, ws_msg.payload);
                    }
                }
            }
            // 处理未知或未实现的消息类型
            unknown_type => {
                // **日志标识符已更改**
                warn!("[SatControlCenter] (处理消息) 收到未处理的 WebSocket 消息类型: '{}'", unknown_type);
            }
        }
    }

    /// 辅助函数：运行心跳循环。
    /// 这是一个静态方法，在新连接建立后由 `connect` 中的任务 `tokio::spawn` 启动。
    async fn run_heartbeat_loop(
        app_handle: AppHandle, // 用于发送事件 (例如超时断开事件)
        ws_send_channel_clone: Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>, // 发送 Ping
        last_pong_received_at_clone: Arc<RwLock<Option<DateTime<Utc>>>>, // 检查 Pong 超时
        is_connected_status_clone: Arc<RwLock<bool>>, // 检查连接状态，并在超时时设置为 false
        _cloud_assigned_client_id_clone: Arc<RwLock<Option<Uuid>>>, // 保留以备将来使用 (例如 Ping 中携带 client_id)
    ) {
        // **日志标识符已更改**
        info!("[SatControlCenter] (心跳任务) 启动。");
        // 创建一个定时器，每隔 HEARTBEAT_INTERVAL_SECONDS 秒触发一次
        let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS));
            loop {
            interval.tick().await; // 等待下一个心跳间隔

            // 1. 检查连接状态，如果已断开则退出心跳循环
                if !*is_connected_status_clone.read().await {
                // **日志标识符已更改**
                info!("[SatControlCenter] (心跳任务) 检测到连接已断开，退出心跳循环。");
                    break; // 退出循环
                }

            // 2. 检查 Pong 响应是否超时
            let now = Utc::now();
                let last_pong_time = *last_pong_received_at_clone.read().await;
                if let Some(last_pong) = last_pong_time {
                // 如果当前时间距离上次收到 Pong 的时间超过了 心跳间隔 + Pong超时容忍时间
                // 使用 chrono::Duration 进行时间差比较
                if now.signed_duration_since(last_pong) > chrono::Duration::seconds((HEARTBEAT_INTERVAL_SECONDS + PONG_TIMEOUT_SECONDS) as i64) {
                    // **日志标识符已更改**
                    warn!(
                        "[SatControlCenter] (心跳任务) Pong 响应超时 (最后一次收到 Pong 是在: {:?}, 当前时间: {:?})。将触发断开连接处理。",
                        last_pong, now
                    );
                    // 发送连接错误事件到前端，告知用户连接可能已断开
                    // **确保 WsConnectionStatusEvent 定义正确**
                    let event_payload = WsConnectionStatusEvent { 
                        connected: false,
                        error_message: Some("心跳响应超时，连接可能已断开".to_string()),
                        client_id: None, // 断开时清除 ID
                    };
                    // **确保 WS_CONNECTION_STATUS_EVENT 常量定义正确**
                    if let Err(e) = app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, &event_payload) {
                        // **日志标识符已更改**
                        error!("[SatControlCenter] (心跳任务) 发送 Pong 超时断开事件失败: {}", e);
                    }
                    // 将主连接状态设置为 false，这将使得主连接任务 (在 connect 中) 检测到并退出清理
                    *is_connected_status_clone.write().await = false;
                    // **日志标识符已更改**
                    info!("[SatControlCenter] (心跳任务) 因 Pong 超时，已将连接状态设为 false，退出心跳循环。");
                    break; // 退出心跳循环
            }
        } else {
                // 如果从未收到过 Pong (last_pong_received_at 仍为 None)，但连接已建立一段时间，也可能表示有问题
                // 但首次 Ping 可能还未收到响应，所以这里暂时不处理 None 的情况，依赖后续的超时
                // **日志标识符已更改**
                debug!("[SatControlCenter] (心跳任务) 尚未收到任何 Pong 响应。");
            }
            
            // 3. 发送 Ping 消息
            // 构建 PingPayload (通常为空结构体)
            let ping_payload = PingPayload {}; 
            // 使用 WsMessage 构造器创建消息
            match WsMessage::new(PING_MESSAGE_TYPE.to_string(), &ping_payload) {
                Ok(ws_message) => {
                    // **日志标识符已更改**
                    info!(
                        "[SatControlCenter] (心跳任务) 准备发送 Ping 消息 (ID: {}, Type: {}) 到云端...",
                        ws_message.message_id, ws_message.message_type
                    );
                    // 获取发送通道的锁
                    if let Some(sender) = ws_send_channel_clone.lock().await.as_mut() {
                         // 将 WsMessage 序列化为 JSON 字符串
                         match serde_json::to_string(&ws_message) { 
                             Ok(msg_json) => {
                                // 使用 Tokio-Tungstenite 的 Sink 发送 Text 消息
                                match sender.send(TungsteniteMessage::Text(msg_json)).await { 
                                Ok(_) => {
                                        // **日志标识符已更改**
                                        info!("[SatControlCenter] (心跳任务) Ping 消息已成功发送。");
                                        // 发送成功后，可以稍微提前更新 last_pong_received_at 的"预期"时间，
                                        // 或者依赖严格的超时检查。
                                        // 为简单起见，此处不更新，等待 Pong 回复来实际更新。
                                }
            Err(e) => {
                                        // **日志标识符已更改**
                                        error!("[SatControlCenter] (心跳任务) 发送 Ping 消息失败: {}", e);
                                        // 发送失败通常意味着底层连接已断开
                                        // 立即将连接状态设为 false，让主任务退出清理
                                        *is_connected_status_clone.write().await = false;
                                        // **日志标识符已更改**
                                        info!("[SatControlCenter] (心跳任务) 因 Ping 发送失败，已将连接状态设为 false，退出心跳循环。");
                                        break; // 退出循环
                                    }
                                }
                                }
                                Err(e) => {
                                // **日志标识符已更改**
                                error!("[SatControlCenter] (心跳任务) 序列化 Ping WsMessage 失败: {}", e);
                                // 序列化失败是内部错误，不直接退出，但需要记录
                            }
                        }
                    } else {
                        // 如果发送通道不可用 (通常是 None)，说明连接已断开
                        // **日志标识符已更改**
                        warn!("[SatControlCenter] (心跳任务) WebSocket 发送通道不可用，无法发送 Ping。退出心跳循环。");
                        break; // 退出循环
                    }
                }
                Err(e) => {
                    // **日志标识符已更改**
                    error!("[SatControlCenter] (心跳任务) 创建 Ping WsMessage 失败: {}", e);
                    // 创建消息失败是内部错误，记录但不退出循环
                }
            }
        }
        // **日志标识符已更改**
        info!("[SatControlCenter] (心跳任务) 已结束。");
    }

    /// 请求断开当前的 WebSocket 连接。
    ///
    /// 这会设置内部状态以指示断开，并通知正在运行的连接任务退出。
    ///
    /// # 返回
    /// * `Result<(), String>`: 如果请求处理成功，则返回 `Ok(())`。
    pub async fn disconnect(&self) -> Result<(), String> {
        // **日志标识符已更改**
        info!("[SatControlCenter] WebSocketClientService::disconnect 被调用。");
        // 获取连接状态的写锁
        let mut is_connected_guard = self.is_connected_status.write().await;
        // 检查当前是否处于连接状态
        if *is_connected_guard {
            // **日志标识符已更改**
            info!("[SatControlCenter] 当前处于连接状态，准备断开...");
            // 设置状态为 false，这将作为信号被 connect 中的主循环检测到
            *is_connected_guard = false; 
            // 显式释放锁，以便其他任务可以继续（如下面的发送 Close 帧）
            drop(is_connected_guard); 

            // （可选但推荐）尝试向服务器发送一个 Close 帧，进行优雅关闭
            let mut sender_guard = self.ws_send_channel.lock().await;
            if let Some(sender) = sender_guard.as_mut() {
                // **日志标识符已更改**
                info!("[SatControlCenter] 尝试向服务器发送 Close 帧...");
                // 使用 SinkExt 的 close() 方法来发送 Close 帧
                match sender.close().await {
                   // **日志标识符已更改**
                   Ok(_) => info!("[SatControlCenter] Close 帧已发送或请求已发出。"),
                   // **日志标识符已更改**
                   Err(e) => warn!("[SatControlCenter] 发送 Close 帧失败（可能连接已断开）: {}", e),
                }
            }
            // 释放发送通道的锁
            drop(sender_guard);

            // (可选) 也可以立即中止连接任务句柄，但这可能不如让任务自己检测到状态变化后退出优雅。
            // relying on the is_connected flag is generally preferred for graceful shutdown.
             let mut task_guard = self.connection_task_handle.lock().await;
             if let Some(handle) = task_guard.as_ref() {
                 // **日志标识符已更改**
                 info!("[SatControlCenter] (disconnect) 请求中止连接任务...");
                 handle.abort(); // 请求任务取消
             }
             // 不在此处移除句柄 (*task_guard = None;)，让任务自己在退出时清理
             drop(task_guard);


            // 发送断开事件到前端，提供即时反馈
            // 注意：连接任务的清理阶段也会发送最终状态事件，这里发送是为了更快响应用户操作
            // **确保 WsConnectionStatusEvent 定义正确**
            let event_payload = WsConnectionStatusEvent {
                connected: false,
                error_message: Some("客户端主动断开连接".to_string()),
                client_id: None, // 断开时 client_id 无意义
            };
            // **确保 WS_CONNECTION_STATUS_EVENT 常量定义正确**
            if let Err(e) = self.app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, &event_payload) {
                // **日志标识符已更改**
                error!(
                    "[SatControlCenter] 发送 \"主动断开\" 事件到前端失败: {}",
                    e
                );
        } else {
                // **日志标识符已更改**
                info!("[SatControlCenter] 已向前端发送主动断开事件。");
            }
             Ok(()) // 断开请求处理成功
        } else {
             // 如果当前未连接，无需执行断开操作
             drop(is_connected_guard); // 释放锁
             // **日志标识符已更改**
             info!("[SatControlCenter] 当前未连接，无需执行断开操作。");
        Ok(())
        }
    }

    /// 检查当前 WebSocket 是否已连接。
    ///
    /// # 返回
    /// * `bool`: 如果连接状态标志为 `true`，则返回 `true`；否则返回 `false`。
    pub async fn is_connected(&self) -> bool {
        // 读取连接状态标志 (使用读锁)
        *self.is_connected_status.read().await
    }

    /// 向 WebSocket 服务器发送一个 `WsMessage`。
    ///
    /// # 参数
    /// * `message` - 要发送的 `WsMessage` 实例。
    ///
    /// # 返回
    /// * `Result<(), String>`: 如果消息成功序列化并放入发送队列，则返回 `Ok(())`。
    ///   如果当前未连接、序列化失败或发送时发生错误，则返回包含错误描述的 `Err(String)`。
    pub async fn send_ws_message(&self, message: WsMessage) -> Result<(), String> {
        // **日志标识符已更改**
        info!(
            "[SatControlCenter] WebSocketClientService::send_ws_message 调用，准备发送类型: {}, ID: {}", 
            message.message_type, message.message_id
        );
        // 检查当前是否已连接
        if !self.is_connected().await {
            let err_msg = "无法发送消息：WebSocket 未连接。".to_string();
            // **日志标识符已更改**
            error!("[SatControlCenter] {}", err_msg);
            return Err(err_msg);
        }

        // 获取发送通道的互斥锁
        let mut sender_guard = self.ws_send_channel.lock().await;
        // 检查发送通道是否存在 (是否为 Some)
        if let Some(sender) = sender_guard.as_mut() {
            // 将 WsMessage 序列化为 JSON 字符串
            match serde_json::to_string(&message) {
                Ok(msg_json) => {
                    // 使用 Sink 发送 Text 消息
                    match sender.send(TungsteniteMessage::Text(msg_json)).await {
                Ok(_) => {
                            // **日志标识符已更改**
                            info!("[SatControlCenter] 消息 (类型: {}, ID: {}) 已成功发送。", message.message_type, message.message_id);
                            Ok(()) // 发送成功
                }
                Err(e) => {
                            // 发送失败，可能连接已在此过程中断开
                            let err_msg = format!("发送 WebSocket 消息时发生错误: {}", e);
                            // **日志标识符已更改**
                            error!("[SatControlCenter] {}", err_msg);
                            // 标记连接为断开状态，以便主任务进行清理
                            *self.is_connected_status.write().await = false;
                            Err(err_msg) // 返回错误
                        }
                    }
                }
                Err(e) => {
                    // 序列化失败是内部错误
                    let err_msg = format!("序列化 WsMessage 失败: {}", e);
                    // **日志标识符已更改**
                    error!("[SatControlCenter] {}", err_msg);
                    Err(err_msg) // 返回错误
                }
            }
        } else {
            // 如果发送通道是 None，说明连接已断开
            let err_msg = "无法发送消息：WebSocket 发送通道不可用 (可能已断开)。".to_string();
            // **日志标识符已更改**
            error!("[SatControlCenter] {}", err_msg);
            // 确保状态反映为断开
            *self.is_connected_status.write().await = false;
            Err(err_msg) // 返回错误
        }
    }

     /// 获取当前缓存的任务调试状态。
     /// 
     /// # 返回
     /// * `Option<TaskDebugState>`: 如果本地有缓存的状态，则返回其克隆；否则返回 `None`。
     pub async fn get_cached_task_state(&self) -> Option<TaskDebugState> {
        // 使用读锁访问缓存
        self.local_task_state_cache.read().await.clone()
    }
}

// 注意：如果 WebSocketClientService::new 需要 app_handle，则不能直接 Default。
// Tauri 的 State 管理通常在 setup 中创建并 manage，所以 Default 可能不需要。
// impl Default for WebSocketClientService {
//     fn default() -> Self {
//         panic!("WebSocketClientService 不能在没有 AppHandle 的情况下 Default::default() 创建");
//     }
// }