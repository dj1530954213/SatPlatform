// SatControlCenter/src-tauri/src/ws_client/service.rs

//! WebSocket 客户端服务模块，用于管理与云端 WebSocket 服务的连接。

use anyhow::Result;
use log::{debug, error, info}; // 移除了 warn
use rust_websocket_utils::client::transport::{connect_client, ClientConnection, receive_message, ClientWsStream};
use rust_websocket_utils::message::WsMessage;
use tauri::{AppHandle, Emitter}; // 移除了未使用的 Manager
use tokio::sync::{RwLock};
use tokio::sync::Mutex as TokioMutex;
// use url::Url; // 移除了未使用的 Url
use std::sync::Arc;
use futures_util::stream::{SplitSink};
use tokio_tungstenite::tungstenite::protocol::Message as TungsteniteMessage;
use futures_util::SinkExt; // 添加 SinkExt
use chrono::{Utc, DateTime}; // 添加 chrono
use std::time::Duration; // 保留此处的 Duration 导入

use crate::event::{WS_CONNECTION_STATUS_EVENT, WsConnectionStatusEvent, ECHO_RESPONSE_EVENT, EchoResponseEventPayload};
use common_models::{self, ws_payloads::{PING_MESSAGE_TYPE, PONG_MESSAGE_TYPE, PingPayload}}; // 显式导入 PING/PONG 类型 和 PingPayload

// 心跳相关常量
const HEARTBEAT_INTERVAL_SECONDS: u64 = 30;
const PONG_TIMEOUT_SECONDS: u64 = 10; // Pong 应该在 (HEARTBEAT_INTERVAL_SECONDS + PONG_TIMEOUT_SECONDS) 内收到

/// WebSocket 客户端服务
/// 管理与云端 WebSocket 服务的连接、消息发送与接收。
#[derive(Debug)]
pub struct WebSocketClientService {
    ws_send_channel: Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
    cloud_assigned_client_id: Arc<RwLock<Option<String>>>,
    app_handle: AppHandle,
    is_connected_status: Arc<RwLock<bool>>,
    connection_task_handle: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
    // 新增心跳相关状态
    heartbeat_task_handle: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
    last_pong_received_at: Arc<RwLock<Option<DateTime<Utc>>>>,
}

impl WebSocketClientService {
    /// 创建一个新的 WebSocketClientService 实例。
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            ws_send_channel: Arc::new(TokioMutex::new(None)),
            cloud_assigned_client_id: Arc::new(RwLock::new(None)),
            app_handle,
            is_connected_status: Arc::new(RwLock::new(false)),
            connection_task_handle: Arc::new(TokioMutex::new(None)),
            // 初始化心跳字段
            heartbeat_task_handle: Arc::new(TokioMutex::new(None)),
            last_pong_received_at: Arc::new(RwLock::new(None)),
        }
    }

    // --- 静态版本的回调辅助函数 ---
    // 这些函数在 tokio::spawn 的任务中被调用，因此它们不能直接访问 &self。
    // 它们通过参数接收 AppHandle 和相关的状态 Arc<...>

    async fn handle_on_open(
        app_handle: &AppHandle,
        cloud_client_id: Option<String>,
        is_connected_status: &Arc<RwLock<bool>>,
        cloud_assigned_client_id_state: &Arc<RwLock<Option<String>>>,
        // 心跳相关状态的 Arc 引用
        ws_send_channel_clone: Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        heartbeat_task_handle_clone: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: Arc<RwLock<Option<DateTime<Utc>>>>,
    ) {
        log::error!("THIS_IS_HANDLE_ON_OPEN_IN_SAT_CONTROL_CENTER_V123_MARKER"); // 独特的错误日志标记
        info!("(静态回调) WebSocket 连接已打开。Cloud Client ID (如果此时已知): {:?}", cloud_client_id);
        *is_connected_status.write().await = true;
        *cloud_assigned_client_id_state.write().await = cloud_client_id.clone();
        *last_pong_received_at_clone.write().await = Some(Utc::now()); // 初始化 last_pong_received_at

        let event_payload = WsConnectionStatusEvent {
            connected: true,
            error_message: Some("成功连接到云端".to_string()),
            client_id: cloud_client_id,
        };
        if let Err(e) = app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, &event_payload) {
            error!("(静态回调) 发送 Connected 事件到前端失败: {}", e);
        }

        info!("(handle_on_open) 即将调用 start_heartbeat_task_static...");

        // 启动心跳任务
        Self::start_heartbeat_task_static(
            app_handle.clone(),
            ws_send_channel_clone,
            heartbeat_task_handle_clone,
            last_pong_received_at_clone,
            is_connected_status.clone(),
            cloud_assigned_client_id_state.clone(),
        ).await;

        info!("(handle_on_open) start_heartbeat_task_static 调用完成。");
    }

    async fn handle_on_message(
        app_handle: &AppHandle,
        ws_msg: WsMessage,
        _cloud_assigned_client_id_state: &Arc<RwLock<Option<String>>>,
        // 心跳相关状态
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
    ) {
        info!("(静态回调) 从云端收到消息: Type='{}', Payload='{}'", ws_msg.message_type, ws_msg.payload);
        if ws_msg.message_type == common_models::ws_payloads::ECHO_MESSAGE_TYPE {
             match serde_json::from_str::<common_models::ws_payloads::EchoPayload>(&ws_msg.payload) {
                Ok(echo_payload) => {
                    info!("(静态回调) 成功解析 EchoPayload: {:?}", echo_payload);
                    let response_event_payload = EchoResponseEventPayload { content: echo_payload.content };
                    if let Err(e) = app_handle.emit_to("main", ECHO_RESPONSE_EVENT, &response_event_payload) {
                         error!("(静态回调) 发送 {} 到前端失败: {}", ECHO_RESPONSE_EVENT, e);
                    }
                }
                Err(e) => {
                    error!("(静态回调) 解析 EchoPayload 失败: {}. Payload: {}", e, ws_msg.payload);
                }
            }
        } else if ws_msg.message_type == PONG_MESSAGE_TYPE {
            info!("(静态回调) 收到 Pong 消息.");
            *last_pong_received_at_clone.write().await = Some(Utc::now());
        }
    }

    async fn handle_on_close(
        app_handle: &AppHandle,
        reason: Option<String>,
        is_connected_status: &Arc<RwLock<bool>>,
        cloud_assigned_client_id_state: &Arc<RwLock<Option<String>>>,
        ws_send_channel_clone_for_cleanup: &Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        // 心跳相关状态
        heartbeat_task_handle_clone: &Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
    ) {
        let reason_msg = reason.unwrap_or_else(|| "连接已关闭，无特定原因".to_string());
        info!("(静态回调) WebSocket 连接已关闭。原因: {}", reason_msg);
        *is_connected_status.write().await = false;
        *cloud_assigned_client_id_state.write().await = None;
        *ws_send_channel_clone_for_cleanup.lock().await = None; // 清理发送通道

        // 停止心跳任务
        Self::stop_heartbeat_task_static(heartbeat_task_handle_clone, last_pong_received_at_clone).await;

        let event_payload = WsConnectionStatusEvent {
            connected: false,
            error_message: Some(reason_msg),
            client_id: None,
        };
        if let Err(e) = app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, &event_payload) {
            error!("(静态回调) 发送 Disconnected 事件到前端失败: {}", e);
        }
    }

    async fn handle_on_error(
        app_handle: &AppHandle,
        error_message_str: String,
        is_connected_status: &Arc<RwLock<bool>>,
        cloud_assigned_client_id_state: &Arc<RwLock<Option<String>>>,
        ws_send_channel_clone_for_cleanup: &Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        // 心跳相关状态
        heartbeat_task_handle_clone: &Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
    ) {
        error!("(静态回调) WebSocket 连接发生错误: {}", error_message_str);
        *is_connected_status.write().await = false;
        *cloud_assigned_client_id_state.write().await = None;
        *ws_send_channel_clone_for_cleanup.lock().await = None; // 清理发送通道

        // 停止心跳任务
        Self::stop_heartbeat_task_static(heartbeat_task_handle_clone, last_pong_received_at_clone).await;

        let event_payload = WsConnectionStatusEvent {
            connected: false,
            error_message: Some(error_message_str),
            client_id: None,
        };
        if let Err(e) = app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, &event_payload) {
            error!("(静态回调) 发送 Error 事件到前端失败: {}", e);
        }
    }

    /// 启动心跳任务 (静态辅助函数)
    async fn start_heartbeat_task_static(
        app_handle: AppHandle,
        ws_send_channel_clone: Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        heartbeat_task_handle_clone: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: Arc<RwLock<Option<DateTime<Utc>>>>,
        is_connected_status_clone: Arc<RwLock<bool>>, // 用于检查连接状态和触发断开
        cloud_assigned_client_id_clone: Arc<RwLock<Option<String>>>, // 用于清理
    ) {
        info!("(心跳任务 Static) 进入 start_heartbeat_task_static。");
        // ---- Start of task_handle_guard scope for taking existing handle ----
        {
            let mut task_handle_guard = heartbeat_task_handle_clone.lock().await;
            if let Some(existing_handle) = task_handle_guard.take() {
                info!("(心跳任务 Static) 检测到已存在的心跳任务，正在中止...");
                existing_handle.abort();
            }
            // task_handle_guard is dropped here, releasing the lock
        }
        // ---- End of task_handle_guard scope ----

        info!("(心跳任务 Static) 启动心跳任务，间隔 {} 秒.", HEARTBEAT_INTERVAL_SECONDS);
        
        // Clone Arcs that need to be moved into the spawned task
        let app_handle_for_spawn = app_handle.clone();
        let ws_send_channel_for_spawn = ws_send_channel_clone.clone();
        let heartbeat_task_handle_for_spawn = heartbeat_task_handle_clone.clone(); // This Arc instance will be moved
        let last_pong_for_spawn = last_pong_received_at_clone.clone();
        let is_connected_for_spawn = is_connected_status_clone.clone();
        let cloud_id_for_spawn = cloud_assigned_client_id_clone.clone();

        let new_task = tokio::spawn(async move {
            info!("(心跳循环) Task spawned. 进入循环。");
            loop {
                info!("(心跳循环) 循环顶部，即将 sleep {} 秒。", HEARTBEAT_INTERVAL_SECONDS);
                tokio::time::sleep(Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS)).await;
                info!("(心跳循环) Sleep 完成。");

                let connected_status = *is_connected_for_spawn.read().await;
                info!("(心跳循环) 当前连接状态: {}", connected_status);
                if !connected_status {
                    info!("(心跳循环) 检测到连接已断开，停止心跳。");
                    break;
                }

                let now = Utc::now();
                let last_pong_opt = *last_pong_for_spawn.read().await;
                if let Some(last_pong_time) = last_pong_opt {
                    if now.signed_duration_since(last_pong_time).num_seconds() > (HEARTBEAT_INTERVAL_SECONDS + PONG_TIMEOUT_SECONDS) as i64 {
                        error!("(心跳任务) Pong 超时! 上次收到 Pong 是在: {:?}, 当前时间: {:?}. 主动断开连接。", last_pong_time, now);
                        *is_connected_for_spawn.write().await = false;
                        
                        // Use the cloned Arcs for the error handler
                        Self::handle_on_error(
                            &app_handle_for_spawn, // Use cloned app_handle
                            "Pong超时，连接可能已丢失".to_string(),
                            &is_connected_for_spawn,
                            &cloud_id_for_spawn,
                            &ws_send_channel_for_spawn, // Use cloned ws_send_channel
                            &heartbeat_task_handle_for_spawn, // Use cloned heartbeat_task_handle
                            &last_pong_for_spawn, // Use cloned last_pong
                        ).await;
                        break;
                    }
                } 

                info!("(心跳循环) 准备发送 Ping...");
                match WsMessage::new(PING_MESSAGE_TYPE.to_string(), &PingPayload {}) {
                    Ok(ping_message) => {
                        let mut sender_guard = ws_send_channel_for_spawn.lock().await; // Use cloned ws_send_channel
                        if let Some(sender) = sender_guard.as_mut() {
                            match sender.send(TungsteniteMessage::Text(serde_json::to_string(&ping_message).unwrap_or_default())).await {
                                Ok(_) => info!("(心跳循环) Ping 消息已发送 (原为debug)."),
                                Err(e) => {
                                    error!("(心跳循环) 发送 Ping 失败: {}. 连接可能已断开.", e);
                                    *is_connected_for_spawn.write().await = false;
                                    break;
                                }
                            }
                        } else {
                            info!("(心跳循环) 发送通道不可用，无法发送 Ping. 连接可能已断开.");
                            *is_connected_for_spawn.write().await = false;
                            break;
                        }
                    }
                    Err(e) => {
                        error!("(心跳循环) 创建 Ping 消息失败: {}", e);
                    }
                }
            }
            info!("(心跳循环) 任务已结束。");
        });

        // After spawning, re-acquire lock to store the new task handle
        let mut task_handle_guard = heartbeat_task_handle_clone.lock().await;
        *task_handle_guard = Some(new_task);
    }

    /// 停止心跳任务 (静态辅助函数)
    async fn stop_heartbeat_task_static(
        heartbeat_task_handle_clone: &Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
    ) {
        info!("(静态辅助) 请求停止心跳任务...");
        let mut guard = heartbeat_task_handle_clone.lock().await;
        if let Some(handle) = guard.take() { // .take() 会移除 Option 中的值并返回它
            handle.abort();
            info!("(静态辅助) 心跳任务已中止。");
        } else {
            info!("(静态辅助) 没有活动的心跳任务需要中止。");
        }
        *last_pong_received_at_clone.write().await = None; // 清理上次接收Pong的时间
    }

    /// 尝试连接到指定的 WebSocket URL。
    pub async fn connect(&self, url_str: &str) -> Result<(), String> {
        info!("WebSocketClientService: 请求连接到 {}", url_str);

        // 0. 停止可能正在运行的旧心跳任务 (以防万一)
        // 通常在 handle_on_close/error 中处理，但作为防御性措施
        Self::stop_heartbeat_task_static(
            &self.heartbeat_task_handle,
            &self.last_pong_received_at
        ).await;

        // 1. 如果已存在连接任务，先中止它
        if let Some(previous_task_handle) = self.connection_task_handle.lock().await.take() {
            info!("检测到已存在的连接任务，正在中止...");
            previous_task_handle.abort();
            // 确保状态被清理，并通知前端旧连接已关闭
            Self::handle_on_close(
                &self.app_handle,
                Some("发起新的连接，旧连接被中止".to_string()),
                &self.is_connected_status,
                &self.cloud_assigned_client_id,
                &self.ws_send_channel,
                &self.heartbeat_task_handle, // 传递给 handle_on_close 以便清理
                &self.last_pong_received_at,  // 传递给 handle_on_close 以便清理
            ).await;
        }
         *self.is_connected_status.write().await = false; // 确保在尝试新连接前状态为未连接


        // 2. 发送 "AttemptingConnection" 事件
        let attempt_payload = WsConnectionStatusEvent {
            connected: false,
            error_message: Some(format!("尝试连接到 {}", url_str)),
            client_id: None,
        };
        if let Err(e) = self.app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, &attempt_payload) {
            error!("发送 AttemptingConnection 事件到前端失败: {}", e);
        }

        // 3. 尝试连接
        match connect_client(url_str.to_string()).await {
            Ok(client_conn) => {
                info!("成功使用 connect_client 连接到 {}", url_str);
                let ClientConnection { ws_sender, mut ws_receiver } = client_conn;
                
                *self.ws_send_channel.lock().await = Some(ws_sender);
                
                // 连接建立后，立即调用 on_open
                // cloud_assigned_client_id 此时通常为 None，会在后续的注册消息中获得
                Self::handle_on_open(
                    &self.app_handle,
                    None, // client_id 通常在注册后才从云端获取
                    &self.is_connected_status,
                    &self.cloud_assigned_client_id,
                    // 传递心跳所需的状态 Arc
                    self.ws_send_channel.clone(),
                    self.heartbeat_task_handle.clone(),
                    self.last_pong_received_at.clone(),
                ).await;

                // 克隆需要在新任务中使用的 Arc 引用
                let app_handle_clone = self.app_handle.clone();
                let is_connected_status_clone = self.is_connected_status.clone();
                let cloud_assigned_client_id_clone = self.cloud_assigned_client_id.clone();
                let ws_send_channel_clone_for_cleanup = self.ws_send_channel.clone();
                // 克隆心跳相关状态，传递给接收任务中的回调
                let heartbeat_task_handle_clone_for_callbacks = self.heartbeat_task_handle.clone();
                let last_pong_received_at_clone_for_callbacks = self.last_pong_received_at.clone();


                // 4. 启动一个异步任务来处理消息接收和连接生命周期
                let connection_task = tokio::spawn(async move {
                    info!("WebSocket 接收任务已启动。");
                    loop {
                        tokio::select! {
                            biased; // 优先处理 ws_receiver.next()
                            msg_option = receive_message(&mut ws_receiver) => {
                                match msg_option {
                                    Some(Ok(ws_msg)) => {
                                        // 为 handle_on_message 传递 last_pong_received_at_clone
                                        Self::handle_on_message(&app_handle_clone, ws_msg, &cloud_assigned_client_id_clone, &last_pong_received_at_clone_for_callbacks).await;
                                    }
                                    Some(Err(ws_err)) => {
                                        let err_msg = format!("接收消息时发生错误: {:?}", ws_err);
                                        // 为 handle_on_error 传递心跳相关的 Arc
                                        Self::handle_on_error(&app_handle_clone, err_msg, &is_connected_status_clone, &cloud_assigned_client_id_clone, &ws_send_channel_clone_for_cleanup, &heartbeat_task_handle_clone_for_callbacks, &last_pong_received_at_clone_for_callbacks).await;
                                        break; // 发生错误，终止接收循环
                                    }
                                    None => { // 流结束，表示连接已关闭
                                        // 为 handle_on_close 传递心跳相关的 Arc
                                        Self::handle_on_close(&app_handle_clone, Some("连接由对方关闭".to_string()), &is_connected_status_clone, &cloud_assigned_client_id_clone, &ws_send_channel_clone_for_cleanup, &heartbeat_task_handle_clone_for_callbacks, &last_pong_received_at_clone_for_callbacks).await;
                                        break; // 连接关闭，终止接收循环
                                    }
                                }
                            }
                            _ = tokio::time::sleep(Duration::from_secs(u64::MAX)) => {
                                // 这个分支理论上不会被执行，除非 receive_message 永久阻塞且没有中止信号
                                // 主要是为了让 tokio::select! 编译通过
                            }
                        }
                    }
                    info!("WebSocket 接收任务已结束。");
                    // 任务结束时（无论是正常关闭还是错误），确保状态被清理
                    // 如果不是因为错误或正常关闭跳出循环（例如任务被 abort），这里的清理可能不会执行
                    // 因此，在 abort 任务的地方 (如 disconnect 或新的 connect)也需要调用清理逻辑
                });

                *self.connection_task_handle.lock().await = Some(connection_task);
                Ok(())
            }
            Err(e) => {
                let err_msg = format!("连接到 {} 失败: {:?}", url_str, e);
                Self::handle_on_error(
                    &self.app_handle,
                    err_msg.clone(),
                    &self.is_connected_status,
                    &self.cloud_assigned_client_id,
                    &self.ws_send_channel,
                    &self.heartbeat_task_handle, // 传递给 handle_on_error
                    &self.last_pong_received_at,  // 传递给 handle_on_error
                ).await;
                Err(err_msg)
            }
        }
    }

    /// 主动断开 WebSocket 连接。
    pub async fn disconnect(&self) -> Result<(), String> {
        info!("WebSocketClientService: 请求断开连接...");

        // 1. 停止心跳任务
        Self::stop_heartbeat_task_static(
            &self.heartbeat_task_handle,
            &self.last_pong_received_at
        ).await;
        
        // 2. 中止主连接任务 (如果存在)
        if let Some(task_handle) = self.connection_task_handle.lock().await.take() {
            info!("正在中止活动的连接任务...");
            task_handle.abort(); 
            // abort 会导致 spawned 任务中的 select! 结束或 receive_message 返回 (取决于实现)
            // 从而触发其内部的 on_close 或 on_error 逻辑。
            // 为确保状态立即更新和事件发送，可以额外调用一次 handle_on_close。
            // 注意：handle_on_close 内部也会调用 stop_heartbeat_task_static，但由于其幂等性，重复调用是安全的。
            Self::handle_on_close(
                &self.app_handle,
                Some("用户请求断开连接".to_string()),
                &self.is_connected_status,
                &self.cloud_assigned_client_id,
                &self.ws_send_channel,
                &self.heartbeat_task_handle, // 传递，虽然刚停止过，但为了回调签名一致和幂等性
                &self.last_pong_received_at,  // 同上
            ).await;
        } else {
            // 如果没有活动的连接任务，但可能仍然处于"已连接"状态 (例如，任务异常结束但状态未清理)
            // 或者只是想确保所有状态都被重置
            info!("没有活动的连接任务，但仍执行清理和状态重置。");
            if *self.is_connected_status.read().await { // 只有在状态为已连接时才发送事件
                 Self::handle_on_close( // 调用以确保状态清理和事件发送
                    &self.app_handle,
                    Some("用户请求断开连接 (无活动任务)".to_string()),
                    &self.is_connected_status,
                    &self.cloud_assigned_client_id,
                    &self.ws_send_channel,
                    &self.heartbeat_task_handle,
                    &self.last_pong_received_at,
                ).await;
            } else { // 如果已经是未连接状态，确保 ws_send_channel 和其他状态被清理
                 *self.ws_send_channel.lock().await = None;
                 *self.cloud_assigned_client_id.write().await = None;
                 // is_connected_status 已经是 false
            }
        }
        
        info!("断开连接请求处理完毕。");
        Ok(())
    }

    /// 检查 WebSocket 是否已连接。
    pub async fn is_connected(&self) -> bool {
        *self.is_connected_status.read().await && self.ws_send_channel.lock().await.is_some()
    }

    /// 向云端发送 WsMessage。
    pub async fn send_ws_message(&self, message: WsMessage) -> Result<(), String> {
        if !self.is_connected().await {
            return Err("WebSocket 未连接，无法发送消息".to_string());
        }

        let mut sender_guard = self.ws_send_channel.lock().await;
        if let Some(sender) = sender_guard.as_mut() {
            let msg_json = serde_json::to_string(&message)
                .map_err(|e| format!("消息序列化失败: {}", e))?;
            
            debug!("客户端服务: 尝试发送消息: {}", msg_json);
            sender.send(TungsteniteMessage::Text(msg_json)).await
                .map_err(|e| {
                    let err_msg = format!("通过 WebSocket 发送消息失败: {}", e);
                    error!("{}", err_msg);
                    // 发送失败可能意味着连接已断开，需要清理状态
                    // 但这里不直接调用 on_close/on_error，因为接收循环应该会检测到
                    err_msg
                })?;
            Ok(())
        } else {
            Err("WebSocket 发送通道不可用 (可能在尝试发送时连接已断开)".to_string())
        }
    }
}

// 确保 Default impl 已被移除
// 如果之前的 edit_file 没有移除它，这里确保它不存在。
// 正确的做法是在 main.rs 的 setup 钩子中使用 WebSocketClientService::new(app_handle) 来创建实例
// 并通过 app.manage() 放入 Tauri State。 