// SatOnSiteMobile/src-tauri/src/ws_client/service.rs

//! `SatOnSiteMobile` (现场端) 应用的 WebSocket 客户端服务模块。
//!
//! 该模块负责管理与云端 `SatCloudService` 的 WebSocket 连接，
//! 包括连接建立、断开、消息收发、状态同步以及心跳维持。

use anyhow::Result;
use log::{debug, error, info};
use rust_websocket_utils::client::transport::{connect_client, ClientConnection, receive_message, ClientWsStream};
use rust_websocket_utils::message::WsMessage;
use tauri::{AppHandle, Emitter};
use tokio::sync::{RwLock};
use tokio::sync::Mutex as TokioMutex;
use std::sync::Arc;
use futures_util::stream::{SplitSink};
use tokio_tungstenite::tungstenite::protocol::Message as TungsteniteMessage;
use futures_util::SinkExt;
use chrono::{Utc, DateTime};
use std::time::Duration;

use crate::event::{WS_CONNECTION_STATUS_EVENT, WsConnectionStatusEvent, ECHO_RESPONSE_EVENT, EchoResponseEventPayload};
use common_models::{self, ws_payloads::{PING_MESSAGE_TYPE, PONG_MESSAGE_TYPE, PingPayload}};

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
    /// 云端分配给此客户端的唯一标识符。
    ///
    /// 在成功连接并注册后，由云端分配。
    /// 使用 `Arc<RwLock<Option<...>>>` 以便在异步回调中安全地更新和读取。
    cloud_assigned_client_id: Arc<RwLock<Option<String>>>,
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
        info!("[SatOnSiteMobile] WebSocketClientService: 正在初始化...");
        Self {
            ws_send_channel: Arc::new(TokioMutex::new(None)),
            cloud_assigned_client_id: Arc::new(RwLock::new(None)),
            app_handle, // 存储传入的 app_handle
            is_connected_status: Arc::new(RwLock::new(false)),
            connection_task_handle: Arc::new(TokioMutex::new(None)),
            heartbeat_task_handle: Arc::new(TokioMutex::new(None)),
            last_pong_received_at: Arc::new(RwLock::new(None)),
        }
    }

    // --- 静态版本的 transport layer 回调辅助函数 ---
    // 这些函数被设计为静态的，以便可以作为参数传递给 `connect_client`，
    // 同时通过克隆和传递必要的 Arc<State> 来访问和修改服务状态。

    /// WebSocket 连接成功打开时的静态回调处理函数。
    ///
    /// 此函数在 `connect_client` 内部的连接成功建立后被调用。
    /// 它会更新连接状态，存储云端分配的客户端ID（如果提供），
    /// 发送连接成功事件到前端，并启动心跳任务。
    ///
    /// # 参数
    /// * `app_handle` - Tauri 应用句柄。
    /// * `cloud_client_id` - 云端在连接时可能分配的客户端 ID。
    /// * `is_connected_status` - 指向服务内部连接状态的 `Arc<RwLock<bool>>`。
    /// * `cloud_assigned_client_id_state` - 指向服务内部存储云端客户端ID的 `Arc<RwLock<Option<String>>>`。
    /// * `ws_send_channel_clone` - 指向服务内部发送通道的 `Arc<TokioMutex<Option<SplitSink>>>`，用于心跳任务。
    /// * `heartbeat_task_handle_clone` - 指向心跳任务句柄的 `Arc<TokioMutex<Option<JoinHandle>>>`。
    /// * `last_pong_received_at_clone` - 指向最后收到Pong时间的 `Arc<RwLock<Option<DateTime>>>`。
    async fn handle_on_open(
        app_handle: &AppHandle,
        cloud_client_id: Option<String>,
        is_connected_status: &Arc<RwLock<bool>>,
        cloud_assigned_client_id_state: &Arc<RwLock<Option<String>>>,
        ws_send_channel_clone: Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        heartbeat_task_handle_clone: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: Arc<RwLock<Option<DateTime<Utc>>>>,
    ) {
        info!("[SatOnSiteMobile] (静态回调) WebSocket 连接已成功打开。云端客户端ID (若连接时分配): {:?}", cloud_client_id);
        *is_connected_status.write().await = true;
        *cloud_assigned_client_id_state.write().await = cloud_client_id.clone(); // 克隆 cloud_client_id 以存储
        *last_pong_received_at_clone.write().await = Some(Utc::now()); // 初始化Pong接收时间

        // 构建并发送连接状态事件到前端
        let event_payload = WsConnectionStatusEvent {
            connected: true,
            error_message: Some("成功连接到云端".to_string()), // 提供更友好的中文消息
            client_id: cloud_client_id.clone(), // 再次克隆 cloud_client_id 用于事件
        };
        if let Err(e) = app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, &event_payload) {
            error!("[SatOnSiteMobile] (静态回调) 发送"已连接"事件到前端失败: {}", e);
        }

        info!("[SatOnSiteMobile] (handle_on_open) 准备启动心跳任务...");
        // 启动心跳任务
        Self::start_heartbeat_task_static(
            app_handle.clone(), // 克隆 app_handle 以传递给新任务
            ws_send_channel_clone, // 传递 Arc 引用
            heartbeat_task_handle_clone, // 传递 Arc 引用
            last_pong_received_at_clone, // 传递 Arc 引用
            is_connected_status.clone(), // 克隆 Arc 引用
            cloud_assigned_client_id_state.clone(), // 克隆 Arc 引用
        ).await;

        info!("[SatOnSiteMobile] (handle_on_open) 心跳任务启动流程调用完成。");
    }

    /// 从云端接收到 WebSocket 消息时的静态回调处理函数。
    ///
    /// 此函数在 `connect_client` 内部的 `receive_message` 任务接收到新消息时被调用。
    /// 它会根据消息类型进行分发处理，例如处理 Echo 回复或 Pong 消息。
    ///
    /// # 参数
    /// * `app_handle` - Tauri 应用句柄。
    /// * `ws_msg` - 从云端接收到的 `WsMessage`。
    /// * `_cloud_assigned_client_id_state` - (当前未使用) 指向存储云端客户端ID的 `Arc`。
    /// * `last_pong_received_at_clone` - 指向最后收到Pong时间的 `Arc<RwLock<Option<DateTime>>>`，用于更新Pong时间。
    async fn handle_on_message(
        app_handle: &AppHandle,
        ws_msg: WsMessage,
        _cloud_assigned_client_id_state: &Arc<RwLock<Option<String>>>, // 参数保留，但目前未使用
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
    ) {
        info!("[SatOnSiteMobile] (静态回调) 从云端收到消息: 类型='{}', 内容摘要='{}'", ws_msg.message_type, ws_msg.payload.chars().take(100).collect::<String>()); // 限制Payload长度以避免日志过长
        
        // 根据消息类型进行处理
        if ws_msg.message_type == common_models::ws_payloads::ECHO_MESSAGE_TYPE {
             match serde_json::from_str::<common_models::ws_payloads::EchoPayload>(&ws_msg.payload) {
                Ok(echo_payload) => {
                    info!("[SatOnSiteMobile] (静态回调) 成功解析 EchoPayload: {:?}", echo_payload);
                    let response_event_payload = EchoResponseEventPayload { content: echo_payload.content };
                    // 将 Echo 回复事件发送到前端
                    if let Err(e) = app_handle.emit_to("main", ECHO_RESPONSE_EVENT, &response_event_payload) {
                         error!("[SatOnSiteMobile] (静态回调) 发送 Echo响应 ({}) 事件到前端失败: {}", ECHO_RESPONSE_EVENT, e);
                    }
                }
                Err(e) => {
                    error!("[SatOnSiteMobile] (静态回调) 解析 EchoPayload 失败: {}. 原始Payload: {}", e, ws_msg.payload);
                }
            }
        } else if ws_msg.message_type == PONG_MESSAGE_TYPE {
            info!("[SatOnSiteMobile] (静态回调) 收到 Pong 消息。");
            // 更新最后收到 Pong 的时间
            *last_pong_received_at_clone.write().await = Some(Utc::now());
        }
        // TODO: 在 P4.1.1 及后续步骤中，此处将添加对 "RegisterResponse", "PartnerStatusUpdate", "TaskStateUpdate" 等消息的处理逻辑。
    }

    /// WebSocket 连接关闭时的静态回调处理函数。
    ///
    /// 此函数在 `connect_client` 内部检测到连接关闭（无论是主动断开还是意外断开）时被调用。
    /// 它会更新连接状态，清理资源（如发送通道），停止心跳任务，并发送连接断开事件到前端。
    ///
    /// # 参数
    /// * `app_handle` - Tauri 应用句柄。
    /// * `reason` - 可选的连接关闭原因。
    /// * `is_connected_status` - 指向服务内部连接状态的 `Arc<RwLock<bool>>`。
    /// * `cloud_assigned_client_id_state` - 指向服务内部存储云端客户端ID的 `Arc<RwLock<Option<String>>>`。
    /// * `ws_send_channel_clone_for_cleanup` - 指向服务内部发送通道的 `Arc<TokioMutex<Option<SplitSink>>>`，用于清理。
    /// * `heartbeat_task_handle_clone` - 指向心跳任务句柄的 `Arc<TokioMutex<Option<JoinHandle>>>`。
    /// * `last_pong_received_at_clone` - 指向最后收到Pong时间的 `Arc<RwLock<Option<DateTime>>>`。
    async fn handle_on_close(
        app_handle: &AppHandle,
        reason: Option<String>,
        is_connected_status: &Arc<RwLock<bool>>,
        cloud_assigned_client_id_state: &Arc<RwLock<Option<String>>>,
        ws_send_channel_clone_for_cleanup: &Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        heartbeat_task_handle_clone: &Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
    ) {
        let reason_msg = reason.unwrap_or_else(|| "连接已关闭，无特定原因".to_string());
        info!("[SatOnSiteMobile] (静态回调) WebSocket 连接已关闭。原因: {}", reason_msg);
        
        // 更新状态并清理资源
        *is_connected_status.write().await = false;
        *cloud_assigned_client_id_state.write().await = None;
        *ws_send_channel_clone_for_cleanup.lock().await = None; // 清理发送通道

        // 停止心跳任务
        Self::stop_heartbeat_task_static(heartbeat_task_handle_clone, last_pong_received_at_clone).await;

        // 构建并发送连接断开事件到前端
        let event_payload = WsConnectionStatusEvent {
            connected: false,
            error_message: Some(reason_msg), // 使用从回调获取或默认的原因
            client_id: None,
        };
        if let Err(e) = app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, &event_payload) {
            error!("[SatOnSiteMobile] (静态回调) 发送"已断开连接"事件到前端失败: {}", e);
        }
    }

    /// WebSocket 连接发生错误时的静态回调处理函数。
    ///
    /// 此函数在 `connect_client` 内部捕获到连接相关的错误时被调用。
    /// 它会记录错误，更新连接状态，清理资源，停止心跳任务，并发送错误事件到前端。
    ///
    /// # 参数
    /// (参数与 `handle_on_close` 类似，但 `error_message_str` 是具体的错误信息)
    /// * `app_handle` - Tauri 应用句柄。
    /// * `error_message_str` - 描述错误的字符串。
    /// * `is_connected_status` - 指向服务内部连接状态的 `Arc<RwLock<bool>>`。
    /// * `cloud_assigned_client_id_state` - 指向服务内部存储云端客户端ID的 `Arc<RwLock<Option<String>>>`。
    /// * `ws_send_channel_clone_for_cleanup` - 指向服务内部发送通道的 `Arc<TokioMutex<Option<SplitSink>>>`，用于清理。
    /// * `heartbeat_task_handle_clone` - 指向心跳任务句柄的 `Arc<TokioMutex<Option<JoinHandle>>>`。
    /// * `last_pong_received_at_clone` - 指向最后收到Pong时间的 `Arc<RwLock<Option<DateTime>>>`。
    async fn handle_on_error(
        app_handle: &AppHandle,
        error_message_str: String,
        is_connected_status: &Arc<RwLock<bool>>,
        cloud_assigned_client_id_state: &Arc<RwLock<Option<String>>>,
        ws_send_channel_clone_for_cleanup: &Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        heartbeat_task_handle_clone: &Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
    ) {
        error!("[SatOnSiteMobile] (静态回调) WebSocket 连接发生错误: {}", error_message_str);
        
        // 更新状态并清理资源
        *is_connected_status.write().await = false;
        *cloud_assigned_client_id_state.write().await = None;
        *ws_send_channel_clone_for_cleanup.lock().await = None; // 清理发送通道

        // 停止心跳任务
        Self::stop_heartbeat_task_static(heartbeat_task_handle_clone, last_pong_received_at_clone).await;

        // 构建并发送错误事件到前端
        let event_payload = WsConnectionStatusEvent {
            connected: false,
            error_message: Some(error_message_str), // 使用从回调获取的错误信息
            client_id: None,
        };
        if let Err(e) = app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, &event_payload) {
            error!("[SatOnSiteMobile] (静态回调) 发送"连接错误"事件到前端失败: {}", e);
        }
    }
    
    /// 启动心跳任务的静态辅助函数。
    ///
    /// 此函数会 `tokio::spawn` 一个新的异步任务，该任务定期发送 Ping 消息，
    /// 并检查是否在超时期限内收到了 Pong 回复。如果超时未收到 Pong，
    /// 则认为连接可能已死，并会调用 `handle_on_error` 来处理断连。
    ///
    /// # 参数
    /// * `app_handle` - Tauri 应用句柄，用于在超时断连时调用 `handle_on_error`。
    /// * `ws_send_channel_clone` - 发送 WebSocket 消息的通道。
    /// * `heartbeat_task_handle_clone` - 用于存储新创建的心跳任务的句柄。
    /// * `last_pong_received_at_clone` - 存储最后收到 Pong 消息的时间戳。
    /// * `is_connected_status_clone` - 当前连接状态。
    /// * `cloud_assigned_client_id_clone` - 云端分配的客户端 ID。
    async fn start_heartbeat_task_static(
        app_handle: AppHandle,
        ws_send_channel_clone: Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        heartbeat_task_handle_clone: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: Arc<RwLock<Option<DateTime<Utc>>>>,
        is_connected_status_clone: Arc<RwLock<bool>>,
        cloud_assigned_client_id_clone: Arc<RwLock<Option<String>>>,
    ) {
        info!("[SatOnSiteMobile] (心跳任务) 正在尝试启动心跳任务...");
        
        // 获取心跳任务句柄的锁，准备存储新的任务句柄
        let mut task_handle_guard = heartbeat_task_handle_clone.lock().await;
        // 如果已存在一个心跳任务，先中止它
        if let Some(existing_handle) = task_handle_guard.take() {
            info!("[SatOnSiteMobile] (心跳任务) 检测到已存在的心跳任务，正在中止旧任务...");
            existing_handle.abort(); // 中止旧任务
            info!("[SatOnSiteMobile] (心跳任务) 旧的心跳任务已中止。");
        }

        info!("[SatOnSiteMobile] (心跳任务) 准备启动新的心跳循环，Ping 发送间隔: {} 秒, Pong 等待超时: {} 秒.", HEARTBEAT_INTERVAL_SECONDS, PONG_TIMEOUT_SECONDS);
        
        // 克隆需要在新异步任务中使用的 Arc 引用
        let app_handle_for_spawn = app_handle.clone();
        let ws_send_channel_for_spawn = ws_send_channel_clone.clone();
        // 注意：heartbeat_task_handle_for_spawn 不需要在这里克隆用于内部，因为它自身就是被管理的句柄容器
        let last_pong_for_spawn = last_pong_received_at_clone.clone();
        let is_connected_for_spawn = is_connected_status_clone.clone();
        let cloud_id_for_spawn = cloud_assigned_client_id_clone.clone();
        let ws_send_channel_for_error_cleanup = ws_send_channel_clone.clone(); // 用于错误处理时的清理
        let heartbeat_task_handle_for_error_cleanup = heartbeat_task_handle_clone.clone(); // 用于错误处理时的清理
        let last_pong_for_error_cleanup = last_pong_received_at_clone.clone(); // 用于错误处理时的清理


        let new_task = tokio::spawn(async move {
            info!("[SatOnSiteMobile] (心跳循环) 心跳任务已启动，进入循环。");
            loop {
                // 1. 等待心跳间隔
                info!("[SatOnSiteMobile] (心跳循环) 等待 {} 秒后发送 Ping...", HEARTBEAT_INTERVAL_SECONDS);
                tokio::time::sleep(Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS)).await;
                
                // 2. 检查连接状态，如果未连接则退出心跳循环
                if !*is_connected_for_spawn.read().await {
                    info!("[SatOnSiteMobile] (心跳循环) 检测到连接已断开，准备退出心跳循环。");
                    break;
                }

                // 3. 检查 Pong 超时
                let current_time = Utc::now();
                let last_pong_time_opt = *last_pong_for_spawn.read().await;
                if let Some(last_pong_time) = last_pong_time_opt {
                    if current_time.signed_duration_since(last_pong_time).num_seconds() as u64 > PONG_TIMEOUT_SECONDS {
                        let error_msg = format!(
                            "Pong超时！超过 {} 秒未收到 Pong 消息。上次收到 Pong 时间: {:?}, 当前时间: {:?}",
                            PONG_TIMEOUT_SECONDS, last_pong_time, current_time
                        );
                        error!("[SatOnSiteMobile] (心跳循环) {}", error_msg);
                        // 调用 handle_on_error 处理断连
                        Self::handle_on_error(
                            &app_handle_for_spawn,
                            error_msg,
                            &is_connected_for_spawn,
                            &cloud_id_for_spawn,
                            &ws_send_channel_for_error_cleanup, // 使用为错误清理准备的克隆
                            &heartbeat_task_handle_for_error_cleanup, // 使用为错误清理准备的克隆
                            &last_pong_for_error_cleanup, // 使用为错误清理准备的克隆
                        ).await;
                        info!("[SatOnSiteMobile] (心跳循环) Pong超时处理完成，退出心跳循环。");
                        break; // 超时则退出心跳循环
                    }
                } else {
                    // 这是第一次发送 Ping (或者 last_pong_received_at 被重置了)
                    // 此时不判断超时，因为还没有基准的 Pong 时间
                    info!("[SatOnSiteMobile] (心跳循环) last_pong_received_at 为 None，本次不检查 Pong 超时 (可能是首次 Ping)。");
                }

                // 4. 发送 Ping 消息
                info!("[SatOnSiteMobile] (心跳循环) 准备发送 Ping 消息...");
                let ping_payload = PingPayload {}; // PingPayload 是空结构体
                match serde_json::to_string(&ping_payload) {
                    Ok(payload_str) => {
                        let ws_message = WsMessage::new(
                            PING_MESSAGE_TYPE.to_string(),
                            payload_str,
                            None, // client_id，通常由服务器在接收时确定或客户端不需要指定
                            None, // group_id
                            None, // target_client_id
                        );

                        let mut sender_guard = ws_send_channel_for_spawn.lock().await;
                        if let Some(sender) = sender_guard.as_mut() {
                            match sender.send(TungsteniteMessage::Text(ws_message.to_json_string())).await {
                                Ok(_) => {
                                    info!("[SatOnSiteMobile] (心跳循环) Ping 消息已发送。");
                                }
                                Err(e) => {
                                    error!("[SatOnSiteMobile] (心跳循环) 发送 Ping 消息失败: {}。可能连接已断开。", e);
                                    // 可以在这里考虑是否立即调用 handle_on_error 或等待下一个循环的 Pong 超时检测
                                    // 为避免立即断开，暂时只记录错误，依赖 Pong 超时机制
                                }
                            }
                        } else {
                            error!("[SatOnSiteMobile] (心跳循环) 尝试发送 Ping 时发现发送通道不存在，可能连接已断开。");
                            info!("[SatOnSiteMobile] (心跳循环) 退出心跳循环。");
                            break; // 发送通道不存在，退出循环
                        }
                    }
                    Err(e) => {
                        error!("[SatOnSiteMobile] (心跳循环) 序列化 PingPayload 失败: {}。无法发送 Ping。", e);
                        // 这是一个内部错误，但通常不应导致连接断开，只记录
                    }
                }
                info!("[SatOnSiteMobile] (心跳循环) Ping 消息发送流程结束。");
            } // loop 结束
            info!("[SatOnSiteMobile] (心跳循环) 心跳任务的内部循环已退出。");
        });
        // 将新创建的任务句柄存储起来
        *task_handle_guard = Some(new_task);
        info!("[SatOnSiteMobile] (心跳任务) 新的心跳任务已成功启动并存储句柄。");
    }

    /// 停止心跳任务的静态辅助函数。
    ///
    /// # 参数
    /// * `heartbeat_task_handle_clone` - 指向心跳任务句柄的 `Arc<TokioMutex<Option<JoinHandle>>>`。
    /// * `last_pong_received_at_clone` - 指向最后收到Pong时间的 `Arc<RwLock<Option<DateTime>>>`，用于重置。
    async fn stop_heartbeat_task_static(
        heartbeat_task_handle_clone: &Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
    ) {
        info!("[SatOnSiteMobile] (心跳任务) 正在尝试停止心跳任务...");
        let mut task_handle_guard = heartbeat_task_handle_clone.lock().await;
        if let Some(handle) = task_handle_guard.take() { // .take() 会移除 Option 中的值并返回它
            handle.abort(); // 中止任务
            info!("[SatOnSiteMobile] (心跳任务) 已中止。");
        } else {
            info!("[SatOnSiteMobile] (心跳任务) 无正在运行的心跳任务可停止。");
        }
        *last_pong_received_at_clone.write().await = None; // 重置最后收到 Pong 的时间
        info!("[SatOnSiteMobile] (心跳任务) last_pong_received_at 已重置。");
    }

    /// 连接到指定的 WebSocket URL。
    ///
    /// 此方法会尝试建立到云端 WebSocket 服务的连接。
    /// 连接成功或失败的状态会通过 Tauri 事件通知前端。
    /// 它还会管理现有的连接任务，确保在发起新连接前中止旧的连接任务。
    ///
    /// # 参数
    /// * `url_str` - 要连接的 WebSocket URL 字符串。
    ///
    /// # 返回
    /// * `Result<(), String>` - 如果连接尝试的初始化过程成功，则返回 `Ok(())`；
    ///   如果 URL 解析失败或无法启动连接任务，则返回 `Err(String)` 包含错误信息。
    ///   注意：此处的 `Ok(())` 仅表示连接过程已开始，不代表连接已成功建立。
    ///   实际的连接成功/失败状态通过 `WsConnectionStatusEvent` 事件异步通知。
    pub async fn connect(&self, url_str: &str) -> Result<(), String> {
        info!("[SatOnSiteMobile] WebSocketClientService: 尝试连接到 URL: {}", url_str);

        // 1. 解析 URL
        let url = match url::Url::parse(url_str) {
            Ok(u) => u,
            Err(e) => {
                let err_msg = format!("无效的 WebSocket URL: {}. 错误: {}", url_str, e);
                error!("[SatOnSiteMobile] {}", err_msg);
                // 立即通过事件通知前端连接尝试失败 (因为URL无效，无法继续)
                let event_payload = WsConnectionStatusEvent {
                    connected: false,
                    error_message: Some(err_msg.clone()),
                    client_id: None,
                };
                if let Err(event_err) = self.app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, &event_payload) {
                    error!("[SatOnSiteMobile] 发送 URL无效 连接错误事件到前端失败: {}", event_err);
                }
                return Err(err_msg);
            }
        };

        // 2. 如果已存在连接任务，则先中止它
        info!("[SatOnSiteMobile] 检查是否存在已有的连接任务...");
        let mut existing_task_handle_guard = self.connection_task_handle.lock().await;
        if let Some(task_handle) = existing_task_handle_guard.take() {
            info!("[SatOnSiteMobile] 检测到已存在的连接任务，正在中止旧任务...");
            task_handle.abort();
            info!("[SatOnSiteMobile] 旧的连接任务已中止。");
            // 确保旧连接相关的状态被清理 (特别是 is_connected_status)
            *self.is_connected_status.write().await = false;
            *self.cloud_assigned_client_id.write().await = None;
            // 也需要停止心跳任务，因为它是与特定连接关联的
            Self::stop_heartbeat_task_static(&self.heartbeat_task_handle, &self.last_pong_received_at).await;
            info!("[SatOnSiteMobile] 旧连接相关的状态已清理，心跳已停止。");
        }

        // 3. 准备回调中会用到的状态 Arc 引用 (克隆)
        let app_handle_clone = self.app_handle.clone();
        let ws_send_channel_clone = self.ws_send_channel.clone();
        let cloud_assigned_client_id_clone = self.cloud_assigned_client_id.clone();
        let is_connected_status_clone = self.is_connected_status.clone();
        let heartbeat_task_handle_clone_for_on_open = self.heartbeat_task_handle.clone();
        let last_pong_received_at_clone_for_callbacks = self.last_pong_received_at.clone();
        
        // 为 on_close 和 on_error 回调准备清理用的克隆
        let ws_send_channel_clone_for_cleanup = self.ws_send_channel.clone();
        let heartbeat_task_handle_clone_for_cleanup = self.heartbeat_task_handle.clone();

        // 4. 定义回调闭包
        let on_open_callback = {
            // 再次克隆需要在 on_open 异步块中使用的 Arc 引用
            let app_handle_for_on_open = app_handle_clone.clone();
            let is_connected_status_for_on_open = is_connected_status_clone.clone();
            let cloud_id_for_on_open = cloud_assigned_client_id_clone.clone();
            let ws_send_for_on_open = ws_send_channel_clone.clone(); // 这个 ws_send_channel_clone 实际上是给心跳任务用的
            let heartbeat_for_on_open = heartbeat_task_handle_clone_for_on_open.clone();
            let last_pong_for_on_open = last_pong_received_at_clone_for_callbacks.clone();

            move |client_connection: ClientConnection| {
                let app_handle_async = app_handle_for_on_open.clone();
                let is_connected_async = is_connected_status_for_on_open.clone();
                let cloud_id_async = cloud_id_for_on_open.clone();
                let ws_send_async = ws_send_for_on_open.clone(); 
                let heartbeat_async = heartbeat_for_on_open.clone();
                let last_pong_async = last_pong_for_on_open.clone();

                async move {
                    info!("[SatOnSiteMobile] (connect) on_open_callback 触发。");
                    // 存储发送通道的 Sink 部分
                    *ws_send_async.lock().await = Some(client_connection.ws_tx);
                    // 调用静态回调处理函数
                    Self::handle_on_open(
                        &app_handle_async,
                        client_connection.client_id, // 从 ClientConnection 获取 client_id
                        &is_connected_async,
                        &cloud_id_async,
                        ws_send_async, // 这个参数实际是给心跳任务内部发送ping用的
                        heartbeat_async,
                        last_pong_async,
                    ).await;
                }
            }
        };

        let on_message_callback = {
            let app_handle_for_on_msg = app_handle_clone.clone();
            let cloud_id_for_on_msg = cloud_assigned_client_id_clone.clone();
            let last_pong_for_on_msg = last_pong_received_at_clone_for_callbacks.clone();
            move |ws_msg: WsMessage| {
                let app_handle_async = app_handle_for_on_msg.clone();
                let cloud_id_async = cloud_id_for_on_msg.clone();
                let last_pong_async = last_pong_for_on_msg.clone();
                async move {
                    info!("[SatOnSiteMobile] (connect) on_message_callback 触发。");
                    Self::handle_on_message(&app_handle_async, ws_msg, &cloud_id_async, &last_pong_async).await;
                }
            }
        };

        let on_close_callback = {
            let app_handle_for_on_close = app_handle_clone.clone();
            let is_connected_for_on_close = is_connected_status_clone.clone();
            let cloud_id_for_on_close = cloud_assigned_client_id_clone.clone();
            let ws_send_for_cleanup = ws_send_channel_clone_for_cleanup.clone();
            let heartbeat_for_cleanup = heartbeat_task_handle_clone_for_cleanup.clone();
            let last_pong_for_cleanup = last_pong_received_at_clone_for_callbacks.clone();
            move |reason: Option<String>| {
                let app_handle_async = app_handle_for_on_close.clone();
                let is_connected_async = is_connected_for_on_close.clone();
                let cloud_id_async = cloud_id_for_on_close.clone();
                let ws_send_async_cleanup = ws_send_for_cleanup.clone();
                let heartbeat_async_cleanup = heartbeat_for_cleanup.clone();
                let last_pong_async_cleanup = last_pong_for_cleanup.clone();
                async move {
                    info!("[SatOnSiteMobile] (connect) on_close_callback 触发。");
                    Self::handle_on_close(
                        &app_handle_async, 
                        reason, 
                        &is_connected_async, 
                        &cloud_id_async, 
                        &ws_send_async_cleanup, 
                        &heartbeat_async_cleanup,
                        &last_pong_async_cleanup,
                    ).await;
                }
            }
        };

        let on_error_callback = {
            // app_handle_clone, is_connected_status_clone, cloud_assigned_client_id_clone 已在外部克隆
            // ws_send_channel_clone_for_cleanup, heartbeat_task_handle_clone_for_cleanup 已在外部克隆
            // last_pong_received_at_clone_for_callbacks 已在外部克隆
            let app_handle_for_on_error = app_handle_clone.clone(); // Renamed for clarity within this scope
            let is_connected_for_on_error = is_connected_status_clone.clone();
            let cloud_id_for_on_error = cloud_assigned_client_id_clone.clone();
            let ws_send_for_error_cleanup = ws_send_channel_clone_for_cleanup.clone(); 
            let heartbeat_for_error_cleanup = heartbeat_task_handle_clone_for_cleanup.clone(); 
            let last_pong_for_error_cleanup = last_pong_received_at_clone_for_callbacks.clone();

            move |err_msg: String| {
                let app_handle_async = app_handle_for_on_error.clone();
                let is_connected_async = is_connected_for_on_error.clone();
                let cloud_id_async = cloud_id_for_on_error.clone();
                let ws_send_async_err_cleanup = ws_send_for_error_cleanup.clone();
                let heartbeat_async_err_cleanup = heartbeat_for_error_cleanup.clone();
                let last_pong_async_err_cleanup = last_pong_for_error_cleanup.clone();
                async move {
                    info!("[SatOnSiteMobile] (connect) on_error_callback 触发。");
                    Self::handle_on_error(
                        &app_handle_async, 
                        err_msg, 
                        &is_connected_async, 
                        &cloud_id_async, 
                        &ws_send_async_err_cleanup, 
                        &heartbeat_async_err_cleanup,
                        &last_pong_async_err_cleanup,
                    ).await;
                }
            }
        };
        
        // 5. 调用 rust_websocket_utils 的 connect_client 启动连接过程
        info!("[SatOnSiteMobile] 准备调用 connect_client 启动 WebSocket 连接任务...");
        let connection_join_handle = connect_client(
            url,
            self.app_handle.clone(), // 主 app_handle 用于 receive_message 内部的日志和潜在的直接事件
            on_open_callback,
            on_message_callback,
            on_close_callback,
            on_error_callback,
            None // client_id_to_register - 客户端通常不预设ID，等待服务端分配或通过业务消息注册
        );

        // 6. 存储新的连接任务句柄
        *existing_task_handle_guard = Some(connection_join_handle);
        info!("[SatOnSiteMobile] 新的 WebSocket 连接任务已启动并存储句柄。");

        Ok(())
    }

    /// 主动断开当前的 WebSocket 连接。
    ///
    /// 此方法会中止正在运行的连接任务和心跳任务，并清理相关状态。
    /// 连接断开的状态会通过 `WsConnectionStatusEvent` 事件通知前端。
    ///
    /// # 返回
    /// * `Result<(), String>` - 如果操作成功（或没有活动连接可断开），返回 `Ok(())`；
    ///   如果内部操作出现问题（理论上不太可能，因为主要是中止任务），则可能返回 `Err`。
    pub async fn disconnect(&self) -> Result<(), String> {
        info!("[SatOnSiteMobile] WebSocketClientService: 尝试主动断开连接...");

        // 1. 中止连接任务
        let mut task_handle_guard = self.connection_task_handle.lock().await;
        if let Some(task_handle) = task_handle_guard.take() { // .take() 会移除 Option 中的值
            info!("[SatOnSiteMobile] 正在中止连接任务...");
            task_handle.abort();
            info!("[SatOnSiteMobile] 连接任务已中止。");
        } else {
            info!("[SatOnSiteMobile] 没有活动的连接任务可中止。");
        }

        // 2. 中止心跳任务 (通过静态辅助函数)
        Self::stop_heartbeat_task_static(&self.heartbeat_task_handle, &self.last_pong_received_at).await;
        // stop_heartbeat_task_static 内部已有日志

        // 3. 清理状态 (部分状态可能已在回调中清理，这里确保彻底)
        *self.is_connected_status.write().await = false;
        *self.cloud_assigned_client_id.write().await = None;
        *self.ws_send_channel.lock().await = None;
        info!("[SatOnSiteMobile] 连接相关状态已清理。");

        // 4. 发送断开连接事件到前端
        // 注意：handle_on_close 或 handle_on_error 回调通常也会发送此事件。
        // 但如果是因为主动 disconnect，那些回调可能不会被 `connect_client` 的逻辑触发。
        // 因此，在这里额外发送一次，确保前端状态一致。可以考虑增加判断避免重复发送。
        let event_payload = WsConnectionStatusEvent {
            connected: false,
            error_message: Some("用户主动断开连接".to_string()),
            client_id: None,
        };
        if let Err(e) = self.app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, &event_payload) {
            error!("[SatOnSiteMobile] 发送"主动断开"事件到前端失败: {}", e);
        } else {
            info!("[SatOnSiteMobile] 已向前端发送"主动断开"事件。");
        }
        
        Ok(())
    }

    /// 检查当前 WebSocket 是否已连接。
    ///
    /// # 返回
    /// * `bool` - 如果已连接则返回 `true`，否则返回 `false`。
    pub async fn is_connected(&self) -> bool {
        *self.is_connected_status.read().await
    }

    /// 通过 WebSocket 连接发送消息到云端。
    ///
    /// # 参数
    /// * `message` - 要发送的 `WsMessage` 实例。
    ///
    /// # 返回
    /// * `Result<(), String>` - 如果消息成功放入发送队列，返回 `Ok(())`；
    ///   如果未连接或发送通道不存在/已关闭，则返回 `Err(String)` 包含错误信息。
    pub async fn send_ws_message(&self, message: WsMessage) -> Result<(), String> {
        let message_type_for_log = message.message_type.clone(); // 克隆用于日志，避免所有权问题
        let payload_summary_for_log = message.payload.chars().take(70).collect::<String>(); // 日志中payload摘要
        info!(
            "[SatOnSiteMobile] 尝试发送 WebSocket 消息: 类型='{}', 内容摘要='{}'", 
            message_type_for_log, 
            payload_summary_for_log
        );

        if !self.is_connected().await {
            let err_msg = "发送消息失败：WebSocket 未连接。".to_string();
            error!("[SatOnSiteMobile] {}", err_msg);
            return Err(err_msg);
        }

        let mut sender_guard = self.ws_send_channel.lock().await;
        if let Some(sender) = sender_guard.as_mut() {
            // 将 WsMessage 序列化为 JSON 字符串
            let msg_json = match message.to_json_string_result() { // 使用 to_json_string_result 获取 Result
                Ok(json_str) => json_str,
                Err(e) => {
                    let err_msg = format!("序列化 WsMessage 失败: {}", e);
                    error!("[SatOnSiteMobile] {}", err_msg);
                    return Err(err_msg);
                }
            };

            // 发送 TungsteniteMessage::Text
            match sender.send(TungsteniteMessage::Text(msg_json)).await {
                Ok(_) => {
                    info!("[SatOnSiteMobile] WebSocket 消息 (类型: {}) 已成功放入发送队列。", message_type_for_log);
                    Ok(())
                }
                Err(e) => {
                    let err_msg = format!("通过 WebSocket 发送消息失败: {}. 可能连接已断开。", e);
                    error!("[SatOnSiteMobile] {}", err_msg);
                    // 考虑在这里也调用 handle_on_error 或 disconnect 来处理状态变更
                    // *self.is_connected_status.write().await = false; // 标记为未连接
                    // Self::stop_heartbeat_task_static(&self.heartbeat_task_handle, &self.last_pong_received_at).await;
                    // self.app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, ...);
                    Err(err_msg)
                }
            }
        } else {
            let err_msg = "发送消息失败：WebSocket 发送通道不存在。可能连接已断开或从未建立。".to_string();
            error!("[SatOnSiteMobile] {}", err_msg);
            Err(err_msg)
        }
    }
}

// Default impl 如果 WebSocketClientService::new 需要 app_handle，则不能直接 Default
// 但 Tauri 的 State 管理通常在 setup 中创建并 manage，所以 Default 可能不需要
// impl Default for WebSocketClientService {
//     fn default() -> Self {
//         panic!("WebSocketClientService 不能在没有 AppHandle 的情况下 Default::default() 创建");
//     }
// } 