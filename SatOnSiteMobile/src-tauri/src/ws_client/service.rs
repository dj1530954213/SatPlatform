// SatOnSiteMobile/src-tauri/src/ws_client/service.rs

//! `SatOnSiteMobile` (现场端) 应用的 WebSocket 客户端服务模块。
//!
//! 该模块负责管理与云端 `SatCloudService` 的 WebSocket 连接，
//! 包括连接建立、断开、消息收发、状态同步以及心跳维持。

use anyhow::Result;
use log::{debug, error, info, warn};
use rust_websocket_utils::client::transport;
use rust_websocket_utils::client::transport::ClientWsStream;
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
use uuid::Uuid;

use crate::event::{
    WS_CONNECTION_STATUS_EVENT, WsConnectionStatusEvent, ECHO_RESPONSE_EVENT, EchoResponseEventPayload,
    WS_REGISTRATION_STATUS_EVENT, WsRegistrationStatusEventPayload,
    WS_PARTNER_STATUS_EVENT, WsPartnerStatusEventPayload,
    LOCAL_TASK_STATE_UPDATED_EVENT, LocalTaskStateUpdatedEventPayload,
    // WS_SERVER_ERROR_EVENT, WsServerErrorEventPayload,
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
        info!("[SatOnSiteMobile] WebSocketClientService: 正在初始化...");
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
        info!("[SatOnSiteMobile] WebSocketClientService::connect 调用，目标 URL: {}", url_str);

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
            info!("[SatOnSiteMobile] 检测到之前的连接任务正在运行，正在尝试取消...");
            handle.abort(); // 请求取消任务
            match handle.await { // 等待任务结束（即使是被取消的）
                Ok(_) => info!("[SatOnSiteMobile] 之前的连接任务已成功取消或完成。"),
                Err(e) if e.is_cancelled() => info!("[SatOnSiteMobile] 之前的连接任务已被取消。"),
                Err(e) => warn!("[SatOnSiteMobile] 等待之前的连接任务结束时发生错误: {:?}", e),
            }
            info!("[SatOnSiteMobile] 已清理之前的连接任务句柄。");
            // 清理相关状态
            *is_connected_status_clone.write().await = false;
            *ws_send_channel_clone.lock().await = None;
            *cloud_assigned_client_id_clone.write().await = None;
            // 心跳任务句柄也应停止
            let mut hb_task_guard = heartbeat_task_handle_clone.lock().await;
            if let Some(hb_handle) = hb_task_guard.take() {
                 hb_handle.abort();
                 info!("[SatOnSiteMobile] 已请求取消相关的心跳任务。");
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
            info!("[SatOnSiteMobile] (连接任务) 开始尝试连接到: {}", url_string);
            match transport::connect_client(url_string.clone()).await {
                Ok(mut client_connection) => {
                    info!("[SatOnSiteMobile] (连接任务) 连接成功建立。");
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
                            "[SatOnSiteMobile] (连接任务) 发送 \"已连接\" 事件 ({}) 失败: {}", 
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
                    loop {
                        tokio::select! {
                            // 接收消息
                            maybe_msg_result = transport::receive_message(&mut client_connection.ws_receiver) => {
                                match maybe_msg_result {
                                    Some(Ok(ws_msg)) => {
                                        // --- 处理收到的消息 (原 handle_on_message 逻辑) ---
                                         Self::process_received_message(
                                             &app_handle_clone,
                                             ws_msg,
                                             &cloud_assigned_client_id_clone,
                                             &last_pong_received_at_clone,
                                             &local_task_state_cache_clone,
                                         ).await;
                                    }
                                    Some(Err(e)) => {
                                        error!("[SatOnSiteMobile] (连接任务) 消息接收或解析时发生错误: {}", e);
                                        final_error_message = Some(format!("接收消息时出错: {}", e));
                                        break; 
                                    }
                                    None => {
                                        info!("[SatOnSiteMobile] (连接任务) WebSocket 连接已由对端关闭。");
                                        final_error_message = Some("连接已由对端关闭".to_string());
                                        break; 
                                    }
                                }
                            }
                            // 检查外部断开信号
                             _ = async { // 异步块，用于检测 is_connected_status_clone 状态的变化
                                 loop {
                                     tokio::time::sleep(Duration::from_millis(500)).await; // 每500毫秒检查一次
                                     // 如果外部状态 (is_connected_status_clone) 变为 false (例如用户调用了 disconnect)
                                     if !*is_connected_status_clone.read().await {
                                         break; // 退出循环，从而使 select! 宏的这个分支被触发
                                     }
                                 }
                             } => {
                                 info!("[现场端移动服务] (连接任务) 检测到连接状态标志变为 false (可能由外部断开调用触发)，主动退出连接任务。");
                                 final_error_message = final_error_message.or_else(|| Some("连接被主动断开".to_string()));
                                 break; // 跳出主 select! 循环，进入清理阶段
                            }
                        }
                    }
                }
                Err(e) => { // WebSocket 连接尝试本身失败 (例如，URL无效，服务器未响应)
                    error!("[现场端移动服务] (连接任务) 尝试连接到 WebSocket 服务器 '{}' 失败: {}", url_string, e);
                    final_error_message = Some(format!("连接到 WebSocket 服务器失败: {}", e));
                    // 不需要 break，因为已经处于循环之外的错误处理路径，将直接进入清理阶段
                }
            }

            // --- 清理阶段 ---
            // 无论连接是正常关闭、发生错误还是被外部中断，都会执行此清理代码块。
            info!("[现场端移动服务] (连接任务) 进入资源清理阶段...");

            // 停止心跳任务
            if let Some(hb_handle) = heartbeat_join_handle {
                info!("[现场端移动服务] (连接任务) 正在中止并等待心跳任务 (heartbeat_task) 结束...");
                hb_handle.abort(); // 请求心跳任务中止
                // 这里可以添加 hb_handle.await 来等待任务实际结束，但 abort() 通常已足够使其停止
                // 清理全局的心跳任务句柄引用
                 let mut hb_guard = heartbeat_task_handle_clone.lock().await;
                 if hb_guard.is_some() { 
                    *hb_guard = None; 
                    info!("[现场端移动服务] (连接任务) 心跳任务句柄已从全局状态中移除。");
                 } 
            }

            // 更新最终的连接状态标记和相关数据
            let was_previously_connected = *is_connected_status_clone.read().await; // 记录断开前的状态
            *is_connected_status_clone.write().await = false; // 明确设置连接状态为 false
            *cloud_assigned_client_id_clone.write().await = None; // 清除已分配的客户端ID
            *ws_send_channel_clone.lock().await = None; // 清除 WebSocket 发送通道
            *last_pong_received_at_clone.write().await = None; // 清除最后接收 Pong 的时间

            // 发送最终的连接状态事件 (WsConnectionStatusEvent) 给前端
            // 仅当连接尝试不成功 (从未连接上) 或之前是连接状态 (现在断开了) 时才发送事件，避免冗余。
            if !connection_attempt_successful || was_previously_connected {
                let event_payload = WsConnectionStatusEvent { 
                    connected: false, // 明确为 false
                    // 提供一个默认的错误/关闭消息，如果 final_error_message 中没有更具体的信息
                    error_message: final_error_message.or_else(|| Some("连接已关闭或意外断开".to_string())), 
                    client_id: None, // 断开后 client_id 无效
                };
                if let Err(e) = app_handle_clone.emit(WS_CONNECTION_STATUS_EVENT, &event_payload) {
                    error!(
                        "[现场端移动服务] (连接任务-清理) 发送连接已关闭或失败事件 ({}) 给前端失败: {}",
                        WS_CONNECTION_STATUS_EVENT, e
                    );
                } else {
                    info!("[现场端移动服务] (连接任务-清理) 已成功发送连接已关闭或失败事件 ({}) 给前端。", WS_CONNECTION_STATUS_EVENT);
                }
            }

            // 从主服务中移除此连接任务的句柄 (如果它仍然是当前的活动任务句柄)
            // 这一步是为了确保如果快速连续调用 connect, 旧的、已结束的连接任务句柄不会被错误地持有。
            // 注意：这里使用了原始的 self.connection_task_handle (通过 connection_task_handle_clone 访问)
            // 而不是当前 tokio::spawn 任务自身的 JoinHandle。
            // 目的是检查全局存储的句柄是否就是刚刚完成的这个任务的句柄。
            let mut task_guard = connection_task_handle_clone.lock().await; // 获取主服务中存储的任务句柄的锁
             if let Some(_current_handle_in_service) = task_guard.as_ref() {
                 // 比较句柄是否指向同一个任务 (JoinHandle 不直接支持 PartialEq, 但可以通过 id 或其他方式间接比较，
                 // 或者更简单地，如果此任务结束时全局句柄仍然是 Some(_)，则通常意味着它是当前句柄)
                 // 为简化，这里假设如果存在句柄，就清除它。更严谨的做法可能需要比较任务ID。
                 info!("[现场端移动服务] (连接任务) 正在从服务状态中移除已完成的连接任务句柄。");
                 *task_guard = None;
             } else {
                 info!("[现场端移动服务] (连接任务) 服务状态中的连接任务句柄已为 None，无需移除 (可能已被新的连接任务覆盖或从未设置)。");
             }
             // MutexGuard 在此作用域结束时自动释放

            info!("[现场端移动服务] (连接任务) 连接处理和清理流程已全部完成。");
        }); // tokio::spawn 的连接任务结束

        // 存储新启动的连接任务的句柄到 self.connection_task_handle
        let mut task_guard = self.connection_task_handle.lock().await;
        *task_guard = Some(connection_task); // 将新创建的后台任务的 JoinHandle 存储起来
        info!("[现场端移动服务] WebSocket 后台连接任务已成功启动，并已存储其句柄。");

        Ok(()) // connect 方法返回 Ok，表示启动连接过程的请求已接受并处理
    }

    /// 辅助函数：处理接收到的单个 WebSocket 消息。
    async fn process_received_message(
        app_handle: &AppHandle,
        ws_msg: WsMessage,
        cloud_assigned_client_id_state: &Arc<RwLock<Option<Uuid>>>,
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
        local_task_state_cache_clone: &Arc<RwLock<Option<TaskDebugState>>>,
    ) {
        info!(
            "[现场端移动服务] (处理消息) 收到消息: 类型='{}', 消息ID='{}', 时间戳='{}', Payload摘要 (前80字符)='{}...'",
            ws_msg.message_type,
            ws_msg.message_id,
            ws_msg.timestamp,
            ws_msg.payload.chars().take(80).collect::<String>() // 获取 Payload 的前80个字符作为摘要
        );

        // --- 根据消息类型分发处理 ---

        // 处理 Pong 消息
        if ws_msg.message_type == PONG_MESSAGE_TYPE {
            info!("[现场端移动服务] (处理消息) 收到来自云端的 Pong 消息。");
            *last_pong_received_at_clone.write().await = Some(Utc::now());
        }
        // 处理 RegisterResponse 消息
        else if ws_msg.message_type == REGISTER_RESPONSE_MESSAGE_TYPE {
            match serde_json::from_str::<RegisterResponsePayload>(&ws_msg.payload) {
                Ok(payload) => {
                    info!(
                        "[现场端移动服务] (处理消息) 成功解析类型为 '{}' 的响应: success={}, msg='{:?}', client_id='{:?}', group_id='{:?}', role='{:?}'",
                        REGISTER_RESPONSE_MESSAGE_TYPE,
                        payload.success, 
                        payload.message, 
                        payload.assigned_client_id, // 这是 Uuid 类型
                        payload.effective_group_id, 
                        payload.effective_role
                    );
                    if payload.success {
                        let client_id_uuid = payload.assigned_client_id;

                        {
                            let mut id_guard = cloud_assigned_client_id_state.write().await;
                            *id_guard = Some(client_id_uuid);
                            info!("[现场端移动服务] (处理消息) 已成功存储云端分配的客户端ID: {}", client_id_uuid);
                        }

                        let conn_status_payload = WsConnectionStatusEvent {
                            connected: true,
                            client_id: Some(client_id_uuid.to_string()), // 转换为 String 给前端
                            error_message: Some("客户端注册成功，已从云端获取并分配客户端ID。".to_string()),
                        };
                        if let Err(e) = app_handle.emit(WS_CONNECTION_STATUS_EVENT, conn_status_payload.clone()) {
                            error!("[现场端移动服务] (处理消息) 发送注册成功后连接状态更新事件 ({}) 给前端失败: {}", WS_CONNECTION_STATUS_EVENT, e);
                        } else {
                            info!("[现场端移动服务] (处理消息) 已成功发送注册成功后连接状态更新事件 ({}) 给前端。", WS_CONNECTION_STATUS_EVENT);
                        }

                        let reg_status_payload = WsRegistrationStatusEventPayload {
                            success: true,
                            message: payload.message.clone(), // 使用云端返回的成功消息
                            assigned_client_id: Some(client_id_uuid.to_string()), 
                            group_id: payload.effective_group_id.clone(), 
                            // TODO: 从 RegisterResponsePayload 中获取 task_id 并填充。
                            // 目前 RegisterResponsePayload (common_models) 定义中可能不直接包含 task_id，
                            // 需要确认云端是否会返回，以及前端是否需要通过此事件获取 task_id。
                            // 如果云端在 RegisterResponsePayload 中返回了 task_id (例如，如果注册时允许 task_id 不同于请求的)
                            // 则应在这里填充: payload.task_id.clone() (假设字段名为 task_id)
                            task_id: None, 
                        };
                        if let Err(e) = app_handle.emit(WS_REGISTRATION_STATUS_EVENT, reg_status_payload.clone()) {
                             error!("[现场端移动服务] (处理消息) 发送WebSocket客户端注册成功事件 ({}) 给前端失败: {}", WS_REGISTRATION_STATUS_EVENT, e);
                         } else {
                            info!("[现场端移动服务] (处理消息) 已成功发送WebSocket客户端注册成功事件 ({}) 给前端。", WS_REGISTRATION_STATUS_EVENT);
                         }

                    } else {
                        // 注册失败
                        warn!(
                            "[现场端移动服务] (处理消息) 收到来自云端的客户端注册失败响应: 原因='{:?}'",
                            payload.message
                        );
                        // 发送注册失败事件给前端
                        let reg_status_payload = WsRegistrationStatusEventPayload {
                            success: false,
                            message: payload.message.clone(), // 使用云端返回的失败消息
                            assigned_client_id: None,
                            group_id: None,
                            task_id: None,
                        };
                         if let Err(e) = app_handle.emit(WS_REGISTRATION_STATUS_EVENT, reg_status_payload.clone()) {
                             error!("[现场端移动服务] (处理消息) 发送WebSocket客户端注册失败事件 ({}) 给前端失败: {}", WS_REGISTRATION_STATUS_EVENT, e);
                         } else {
                            info!("[现场端移动服务] (处理消息) 已成功发送WebSocket客户端注册失败事件 ({}) 给前端。", WS_REGISTRATION_STATUS_EVENT);
                         }
                    }
                }
                Err(e) => { // 反序列化 RegisterResponsePayload 失败
                    error!(
                        "[现场端移动服务] (处理消息) 反序列化来自云端的 '{}' 类型的响应 Payload 失败: {}. 原始Payload: '{}'",
                        REGISTER_RESPONSE_MESSAGE_TYPE, e, ws_msg.payload
                    );
                    // 尝试发送一个通用的连接错误事件，提示处理消息时发生内部错误
                    let current_id_str = (*cloud_assigned_client_id_state.read().await).as_ref().map_or_else(|| "无".to_string(), |id_ref| id_ref.to_string());
                    let error_payload = WsConnectionStatusEvent {
                        connected: cloud_assigned_client_id_state.read().await.is_some(), // 保持当前已知的连接状态
                        client_id: if current_id_str == "无" { None } else { Some(current_id_str) }, // 修正client_id的逻辑
                        error_message: Some(format!("处理来自云端的注册响应消息时发生内部错误: {}", e)),
                    };
                    if let Err(emit_e) = app_handle.emit(WS_CONNECTION_STATUS_EVENT, error_payload.clone()) {
                        error!("[现场端移动服务] (处理消息) 发送处理注册响应失败的连接状态事件 ({}) 给前端失败: {}", WS_CONNECTION_STATUS_EVENT, emit_e);
                    }
                }
            }
        }
        // 处理 PartnerStatusUpdate 消息
        else if ws_msg.message_type == PARTNER_STATUS_UPDATE_MESSAGE_TYPE {
             match serde_json::from_str::<PartnerStatusPayload>(&ws_msg.payload) {
                 Ok(payload) => {
                     info!(
                         "[现场端移动服务] (处理消息) 成功解析类型为 '{}' 的伙伴状态更新: 伙伴角色='{}', 是否在线={}, 伙伴客户端ID='{}', 组ID='{}'",
                         PARTNER_STATUS_UPDATE_MESSAGE_TYPE,
                         payload.partner_role, // ClientRole 枚举，会通过 Debug trait 打印
                         payload.is_online,
                         payload.partner_client_id, // Uuid 类型
                         payload.group_id
                     );
                     // 构造发送给前端的 WsPartnerStatusEventPayload 事件负载
                     // 注意：WsPartnerStatusEventPayload (在 event.rs 定义) 字段需要与此处匹配
                     let event_payload = WsPartnerStatusEventPayload {
                         partner_role: payload.partner_role.to_string(), // ClientRole 枚举转换为字符串
                         is_online: payload.is_online,
                         partner_client_id: Some(payload.partner_client_id.to_string()), // Uuid 转换为字符串
                         group_id: Some(payload.group_id.clone()), // 组ID
                     };
                     if let Err(e) = app_handle.emit(WS_PARTNER_STATUS_EVENT, event_payload.clone()) { 
                          error!("[现场端移动服务] (处理消息) 发送伙伴客户端状态更新事件 ({}) 给前端失败: {}", WS_PARTNER_STATUS_EVENT, e);
                      } else {
                        info!("[现场端移动服务] (处理消息) 已成功发送伙伴客户端状态更新事件 ({}) 给前端。", WS_PARTNER_STATUS_EVENT);
                      }
                 }
                 Err(e) => { // 反序列化 PartnerStatusPayload 失败
                     error!(
                         "[现场端移动服务] (处理消息) 反序列化来自云端的 '{}' 类型的伙伴状态更新 Payload 失败: {}. 原始Payload: '{}'",
                         PARTNER_STATUS_UPDATE_MESSAGE_TYPE, e, ws_msg.payload
                     );
                 }
             }
        }
        // 处理 TaskStateUpdate 消息
        else if ws_msg.message_type == TASK_STATE_UPDATE_MESSAGE_TYPE {
            match serde_json::from_str::<TaskDebugState>(&ws_msg.payload) { // TaskDebugState 来自 common_models
                 Ok(new_state) => {
                     info!(
                         "[现场端移动服务] (处理消息) 成功解析类型为 '{}' 的任务状态更新: 任务ID='{}', 最后更新者='{:?}', 时间戳={}",
                         TASK_STATE_UPDATE_MESSAGE_TYPE,
                         new_state.task_id,
                         new_state.last_updated_by_role,
                         new_state.last_update_timestamp
                     );
                     info!("[现场端移动服务] (处理消息) 收到的完整 TaskDebugState: {:?}", new_state);

                     // 更新本地缓存中的任务状态
                     *local_task_state_cache_clone.write().await = Some(new_state.clone()); // 克隆 new_state 以存入缓存
                     info!("[现场端移动服务] (处理消息) 本地任务状态缓存已更新为来自云端的最新状态 (任务ID: {}).", new_state.task_id);
                     
                     // 构造发送给前端的 LocalTaskStateUpdatedEventPayload 事件负载
                     // LocalTaskStateUpdatedEventPayload (在 event.rs 定义) 的 new_state 字段需要是 serde_json::Value 类型
                     match serde_json::to_value(new_state.clone()) { // 将 TaskDebugState 序列化为 serde_json::Value
                         Ok(state_value) => {
                             let event_payload = LocalTaskStateUpdatedEventPayload { new_state: state_value };
                             if let Err(e) = app_handle.emit(LOCAL_TASK_STATE_UPDATED_EVENT, event_payload.clone()) { 
                                 error!("[现场端移动服务] (处理消息) 发送本地任务状态已更新事件 ({}) 给前端失败: {}", LOCAL_TASK_STATE_UPDATED_EVENT, e);
                             } else {
                                info!("[现场端移动服务] (处理消息) 已成功发送本地任务状态已更新事件 ({}) 给前端。", LOCAL_TASK_STATE_UPDATED_EVENT);
                             }
                         }
                         Err(e) => { // 序列化 TaskDebugState 为 serde_json::Value 失败
                             error!("[现场端移动服务] (处理消息) 序列化 TaskDebugState (任务ID: {}) 为 serde_json::Value 以便发送事件时失败: {}", new_state.task_id, e);
                         }
                     }
                 }
                 Err(e) => { // 反序列化 TaskDebugState 失败
                     error!(
                         "[现场端移动服务] (处理消息) 反序列化来自云端的 '{}' 类型的任务状态更新 Payload 失败: {}. 原始Payload: '{}'",
                         TASK_STATE_UPDATE_MESSAGE_TYPE, e, ws_msg.payload
                     );
                 }
             }
        }
        // 处理 Echo 响应消息
        else if ws_msg.message_type == ECHO_MESSAGE_TYPE { // 注意：通常 Echo 是客户端发往服务端，服务端响应。这里假设是处理服务端的 Echo 响应。
            match serde_json::from_str::<EchoPayload>(&ws_msg.payload) { // EchoPayload 来自 common_models
                Ok(payload) => {
                    info!(
                        "[现场端移动服务] (处理消息) 成功解析类型为 '{}' 的 Echo 响应: 内容='{}'",
                        ECHO_MESSAGE_TYPE, payload.content
                    );
                    // 构造 EchoResponseEventPayload 事件负载并发送给前端
                    let event_payload = EchoResponseEventPayload { content: payload.content }; // content 字段来自 EchoPayload
                    if let Err(e) = app_handle.emit(ECHO_RESPONSE_EVENT, event_payload.clone()) { 
                        error!("[现场端移动服务] (处理消息) 发送 Echo 响应事件给前端失败: {}", e);
                    } else {
                        info!("[现场端移动服务] (处理消息) 已成功发送 Echo 响应事件给前端。");
                    }
                }
                Err(e) => { // 反序列化 EchoPayload 失败
                    error!(
                        "[现场端移动服务] (处理消息) 反序列化来自云端的 '{}' 类型的 Echo 响应 Payload 失败: {}. 原始Payload: '{}'",
                        ECHO_MESSAGE_TYPE, e, ws_msg.payload
                    );
                }
            }
        }
        // 处理来自云端的错误响应消息
        else if ws_msg.message_type == ERROR_RESPONSE_MESSAGE_TYPE {
             match serde_json::from_str::<ErrorResponsePayload>(&ws_msg.payload) { // ErrorResponsePayload 来自 common_models
                 Ok(payload) => {
                     error!(
                         "[现场端移动服务] (处理消息) 收到来自云端的错误响应消息: 错误内容='{}', 原始消息类型可能为='{:?}'",
                         payload.error, // 错误描述信息
                         payload.original_message_type // 触发错误的原始消息类型 (可选)
                     );
                     // 更新连接状态事件，将错误信息传递给前端
                     let current_client_id_str = (*cloud_assigned_client_id_state.read().await).as_ref().map_or_else(|| String::new(), |id_ref| id_ref.to_string());
                     let error_event_payload = WsConnectionStatusEvent {
                        connected: cloud_assigned_client_id_state.read().await.is_some(), // 保持当前已知的连接状态
                        client_id: if current_client_id_str.is_empty() { None } else { Some(current_client_id_str) },
                        error_message: Some(format!("收到来自云端服务的错误报告: {}", payload.error)),
                     };
                     if let Err(e) = app_handle.emit(WS_CONNECTION_STATUS_EVENT, error_event_payload.clone()) {
                        error!("[现场端移动服务] (处理消息) 发送云端错误报告的连接状态事件 ({}) 给前端失败: {}", WS_CONNECTION_STATUS_EVENT, e);
                     }
                 }
                 Err(e) => { // 反序列化 ErrorResponsePayload 失败
                     error!(
                         "[现场端移动服务] (处理消息) 反序列化来自云端的 '{}' 类型的错误响应 Payload 失败: {}. 原始Payload: '{}'",
                         ERROR_RESPONSE_MESSAGE_TYPE, e, ws_msg.payload
                     );
                 }
             }
        }
        // 处理未知的消息类型
        else {
            warn!(
                "[现场端移动服务] (处理消息) 收到未知的 WebSocket 消息类型: '{}'。消息ID: {}, 完整消息: {:?}",
                ws_msg.message_type, ws_msg.message_id, ws_msg
            );
        }
    }

    /// 辅助异步函数：在独立的 Tokio 任务中运行心跳循环。
    ///
    /// 此函数由 `connect_client` 在成功建立 WebSocket 连接后启动。
    ///
    /// # 主要职责:
    /// 1. 定期 (每 `HEARTBEAT_INTERVAL_SECONDS` 秒) 执行以下操作：
    ///    a. 检查 WebSocket 连接是否仍然有效 (`is_connected_status_clone`)。
    ///    b. 检查自上次收到 Pong 消息以来是否已超时 (`PONG_TIMEOUT_SECONDS`)。
    ///       - 如果超时，认为连接已死，设置 `is_connected_status_clone` 为 `false` 以触发主连接任务的清理和退出，并发送连接错误事件给前端。
    ///    c. 如果连接有效且未超时，则构建一个 `PingPayload`，封装在 `WsMessage` 中，并通过 `ws_send_channel_clone` 发送给云端服务器。
    ///       - 如果发送 Ping 失败，也认为连接已断开，设置 `is_connected_status_clone` 为 `false`。
    /// 2. 当检测到连接断开或发送 Ping 失败时，心跳循环会自行终止。
    ///
    /// # 参数:
    /// * `app_handle`: Tauri `AppHandle` 的克隆，用于向前端发送事件 (例如 Pong 超时事件)。
    /// * `ws_send_channel_clone`: 对共享的 WebSocket 发送通道 (`SplitSink`) 的 `Arc<TokioMutex<...>>` 引用。
    /// * `last_pong_received_at_clone`: 对共享状态 `last_pong_received_at` (上次收到 Pong 的时间) 的 `Arc<RwLock<...>>` 引用。
    /// * `is_connected_status_clone`: 对共享的连接状态标志 (`bool`) 的 `Arc<RwLock<...>>` 引用。
    /// * `_cloud_assigned_client_id_clone`: 对云端分配的客户端 ID 的 `Arc<RwLock<...>>` 引用 (当前在此函数中未使用，但保留以备将来扩展)。
    async fn run_heartbeat_loop(
        app_handle: AppHandle,
        ws_send_channel_clone: Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        last_pong_received_at_clone: Arc<RwLock<Option<DateTime<Utc>>>>,
        is_connected_status_clone: Arc<RwLock<bool>>,
        _cloud_assigned_client_id_clone: Arc<RwLock<Option<Uuid>>>, // 参数保留，但当前未使用，加下划线
    ) {
        info!("[现场端移动服务] (心跳任务) 心跳维持任务已启动。心跳间隔: {}秒，Pong超时: {}秒。", HEARTBEAT_INTERVAL_SECONDS, PONG_TIMEOUT_SECONDS);
        // 创建一个固定周期的计时器
        let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS));
            loop {
            // 等待下一个心跳间隔点到达
            interval.tick().await;

            // 1. 检查外部连接状态是否仍然指示为"已连接"
            if !*is_connected_status_clone.read().await {
                info!("[现场端移动服务] (心跳任务) 检测到主连接状态已变为未连接，心跳任务将自动终止。");
                    break; // 退出心跳循环
                }

            // 2. 检查 Pong 响应是否超时
            let now = Utc::now();
            let last_pong_time = *last_pong_received_at_clone.read().await;
            if let Some(last_pong) = last_pong_time { // 如果之前收到过 Pong
                // 计算从上次收到 Pong 到现在的时间差，与 Pong 超时阈值比较
                if now.signed_duration_since(last_pong) > chrono::Duration::seconds((HEARTBEAT_INTERVAL_SECONDS + PONG_TIMEOUT_SECONDS) as i64) {
                    warn!(
                        "[现场端移动服务] (心跳任务) Pong 响应超时！最后一次收到 Pong 是在: {:?} (UTC), 当前时间: {:?} (UTC)。将视作连接断开。",
                        last_pong, now
                    );
                    // 发送连接错误事件给前端，通知 Pong 超时
                    let event_payload = WsConnectionStatusEvent { 
                        connected: false,
                        error_message: Some("与云端服务的心跳响应超时，连接可能已丢失。".to_string()),
                        client_id: None, // 超时后，之前的 client_id 可能已失效
                    };
                    if let Err(e) = app_handle.emit(WS_CONNECTION_STATUS_EVENT, &event_payload) {
                        error!("[现场端移动服务] (心跳任务) 发送Pong响应超时导致连接断开事件 ({}) 给前端失败: {}", WS_CONNECTION_STATUS_EVENT, e);
                    }
                    // 将主连接状态设置为 false，这将使得主连接任务 (connect_client) 中的读取循环和 select! 终止，进而进入清理阶段。
                    *is_connected_status_clone.write().await = false;
                    info!("[现场端移动服务] (心跳任务) 因 Pong 响应超时，已将主连接状态设置为 false，心跳任务即将终止。");
                    break; // 退出心跳循环
                }
            } else {
                // 初始连接后，可能尚未收到第一个 Pong
                debug!("[现场端移动服务] (心跳任务) 当前尚未收到任何 Pong 响应 (可能处于连接初期或上一个 Pong 间隔内)。");
            }
            
            // 3. 如果连接仍然被认为是活动的，则发送 Ping 消息
            let ping_payload = PingPayload {}; // PingPayload 是一个空结构体
            match WsMessage::new(PING_MESSAGE_TYPE.to_string(), &ping_payload) {
                Ok(ws_message) => {
                    info!(
                        "[现场端移动服务] (心跳任务) 准备向云端发送 Ping 消息 (ID: {}, 类型: {}) ...",
                        ws_message.message_id, ws_message.message_type
                    );
                    // 获取对 WebSocket 发送通道的独占访问权限
                    if let Some(sender) = ws_send_channel_clone.lock().await.as_mut() {
                         match serde_json::to_string(&ws_message) { // 将 WsMessage 序列化为 JSON 字符串
                             Ok(msg_json) => {
                                // 发送文本类型的 WebSocket 消息 (Ping)
                                match sender.send(TungsteniteMessage::Text(msg_json)).await { 
                                Ok(_) => {
                                        info!("[现场端移动服务] (心跳任务) Ping 消息 (ID: {}) 已成功发送至云端。", ws_message.message_id);
                                }
                                Err(e) => { // 发送 Ping 失败
                                        error!("[现场端移动服务] (心跳任务) 发送 Ping 消息 (ID: {}) 失败: {}。将视作连接断开。", ws_message.message_id, e);
                                        // 发送失败也应视为连接问题，设置主连接状态为 false
                                        *is_connected_status_clone.write().await = false;
                                        info!("[现场端移动服务] (心跳任务) 因 Ping 消息发送失败，已将主连接状态设置为 false，心跳任务即将终止。");
                                        break; // 退出心跳循环
                                    }
                                }
                            }
                            Err(e) => { // 序列化 WsMessage (用于Ping) 失败
                                error!("[现场端移动服务] (心跳任务) 序列化 Ping 类型的 WsMessage (ID: {}) 失败: {}。将跳过此次 Ping 发送。", ws_message.message_id, e);
                                // 这种内部错误不直接断开连接，但需要记录
                            }
                        }
                    } else {
                        // 如果 ws_send_channel 是 None，说明连接已断开或正在断开
                        warn!("[现场端移动服务] (心跳任务) WebSocket 发送通道当前不可用 (可能连接已关闭)，无法发送 Ping。心跳任务将终止。");
                        break; // 退出心跳循环
                    }
                }
            Err(e) => { // 创建 Ping 消息的 WsMessage 结构体失败
                    error!("[现场端移动服务] (心跳任务) 构建 Ping 类型的 WsMessage 失败: {}。将跳过此次 Ping 发送。", e);
                    // 这种内部错误不直接断开连接，但需要记录
                }
            }
        } // loop 结束
        info!("[现场端移动服务] (心跳任务) 心跳维持任务已结束。");
    }

    /// 客户端主动请求断开当前的 WebSocket 连接。
    ///
    /// # 主要流程:
    /// 1. 检查当前是否已连接 (`is_connected_status`)。
    ///    - 如果未连接，则记录日志并直接返回 `Ok(())`。
    /// 2. 如果已连接：
    ///    a. 将 `is_connected_status` 设置为 `false`。这将作为信号，使得 `connect_client` 中的主读取循环和心跳任务 (`run_heartbeat_loop`) 检测到并开始退出流程。
    ///    b. 尝试向服务器发送一个 WebSocket Close 帧，以优雅地关闭连接。
    ///    c. 中止 (abort) 当前正在运行的连接任务 (`connection_task_handle`)，以确保其彻底停止并执行清理逻辑。
    ///    d. 向前端发送一个 `WsConnectionStatusEvent` 事件，通知连接已主动断开。
    ///
    /// # 返回
    /// * `Result<(), String>`: 通常返回 `Ok(())`。如果发送事件失败，会记录错误但不会使此方法失败。
    pub async fn disconnect(&self) -> Result<(), String> {
        info!("[现场端移动服务] WebSocketClientService::disconnect (主动断开连接) 方法被调用。");
        // 获取写锁以修改连接状态
        let mut is_connected_guard = self.is_connected_status.write().await;
        if *is_connected_guard { // 检查是否真的处于连接状态
            info!("[现场端移动服务] (主动断开) 当前 WebSocket 处于连接状态，将执行断开流程...");
            // 1. 设置共享的连接状态标志为 false。这是给 connect_client 任务和心跳任务的主要信号。
            *is_connected_guard = false; 
            drop(is_connected_guard); // 及时释放写锁，允许其他任务读取更新后的状态

            // 2. 尝试向 WebSocket 服务器发送一个 Close 帧，进行优雅关闭。
            let mut sender_guard = self.ws_send_channel.lock().await;
            if let Some(sender) = sender_guard.as_mut() {
                info!("[现场端移动服务] (主动断开) 尝试向云端 WebSocket 服务器发送 Close 帧...");
                match sender.close().await {
                   Ok(_) => info!("[现场端移动服务] (主动断开) WebSocket Close 帧已成功发送或请求已发出。"),
                   Err(e) => warn!("[现场端移动服务] (主动断开) 尝试发送 WebSocket Close 帧失败 (可能连接此时已从远端断开): {}", e),
                }
            }
            drop(sender_guard); // 释放发送通道的锁

            // 3. 请求中止 (abort) 后台的连接处理任务 (connect_client)。
            // abort() 会使得任务在下一个 .await 点被取消。
             let task_guard = self.connection_task_handle.lock().await;
             if let Some(handle) = task_guard.as_ref() {
                 info!("[现场端移动服务] (主动断开) 请求中止正在运行的 WebSocket 连接任务...");
                 handle.abort();
             }
             drop(task_guard); // 释放任务句柄的锁

            // 4. 向前端发送一个明确的"主动断开"状态事件。
            let event_payload = WsConnectionStatusEvent {
                connected: false,
                error_message: Some("客户端已主动发起断开 WebSocket 连接的操作。".to_string()),
                client_id: None, // 断开后，之前分配的 client_id 通常不再有效
            };
            if let Err(e) = self.app_handle.emit(WS_CONNECTION_STATUS_EVENT, &event_payload) {
                error!(
                    "[现场端移动服务] (主动断开) 发送主动断开连接事件 ({}) 给前端失败: {}",
                    WS_CONNECTION_STATUS_EVENT, e
                );
            } else {
                info!("[现场端移动服务] (主动断开) 已成功发送主动断开连接事件 ({}) 给前端。", WS_CONNECTION_STATUS_EVENT);
            }
             Ok(())
        } else {
             drop(is_connected_guard); // 确保锁被释放
             info!("[现场端移动服务] (主动断开) WebSocket 当前本就未连接，无需执行断开操作。");
            Ok(())
        }
    }

    /// 检查当前 WebSocket 是否已连接。
    ///
    /// # 返回
    /// * `bool`: 如果 `is_connected_status` 状态为 `true`，则返回 `true`；否则返回 `false`。
    pub async fn is_connected(&self) -> bool {
        *self.is_connected_status.read().await // 读取共享的连接状态标志
    }

    /// 向已连接的 WebSocket 服务器异步发送一个 `WsMessage`。
    ///
    /// # 主要流程:
    /// 1. 检查当前是否已连接 (`is_connected()`)。
    ///    - 如果未连接，记录错误并返回 `Err`。
    /// 2. 获取对 `ws_send_channel` (WebSocket 发送端) 的互斥锁。
    /// 3. 如果发送通道存在：
    ///    a. 将传入的 `WsMessage` 对象序列化为 JSON 字符串。
    ///    b. 使用 `sender.send()` 方法将 JSON 字符串作为文本消息发送出去。
    ///    c. 根据发送结果，记录成功或失败日志，并返回相应的 `Result`。
    /// 4. 如果发送通道不存在 (例如，在断开过程中被清空)，记录警告并返回 `Err`。
    ///
    /// # 参数
    /// * `message`: 要发送的 `WsMessage` 实例。
    ///
    /// # 返回
    /// * `Result<(), String>`: 
    ///     - `Ok(())`: 如果消息已成功序列化并交由 WebSocket 发送端处理。
    ///     - `Err(String)`: 如果未连接、序列化失败、或发送过程中发生错误，则返回包含中文错误描述的字符串。
    pub async fn send_ws_message(&self, message: WsMessage) -> Result<(), String> {
        info!(
            "[现场端移动服务] WebSocketClientService::send_ws_message 方法调用，准备发送消息: 类型='{}', ID='{}'", 
            message.message_type, message.message_id
        );
        // 1. 检查连接状态
        if !self.is_connected().await {
            let err_msg = "无法发送 WebSocket 消息：当前未连接到云端服务。".to_string();
            error!("[现场端移动服务] (发送消息) {}", err_msg);
            return Err(err_msg);
        }

        // 2. 获取发送通道的锁
        let mut sender_guard = self.ws_send_channel.lock().await;
        if let Some(sender) = sender_guard.as_mut() { // 3. 检查发送通道是否存在
            // 3a. 序列化 WsMessage 为 JSON 字符串
            match serde_json::to_string(&message) { 
                Ok(msg_json) => {
                    // 3b. 发送消息
                    match sender.send(TungsteniteMessage::Text(msg_json)).await {
                        Ok(_) => {
                            info!(
                                "[现场端移动服务] (发送消息) 消息 (类型: '{}', ID: '{}') 已成功发送至 WebSocket 服务器。",
                                message.message_type, message.message_id
                            );
                            Ok(())
                        }
                        Err(e) => { // 发送失败
                            let err_msg = format!(
                                "发送 WebSocket 消息 (类型: '{}', ID: '{}') 失败: {}. 可能连接已中断。",
                                message.message_type, message.message_id, e
                            );
                            error!("[现场端移动服务] (发送消息) {}", err_msg);
                            // 考虑在这种情况下也设置 is_connected_status 为 false，并通知前端
                            // *self.is_connected_status.write().await = false;
                            // self.app_handle.emit(...);
                            Err(err_msg)
                        }
                    }
                }
                Err(e) => { // 序列化失败
                    let err_msg = format!(
                        "序列化 WebSocket 消息 (类型: '{}', ID: '{}') 为 JSON 时失败: {}",
                        message.message_type, message.message_id, e
                    );
                    error!("[现场端移动服务] (发送消息) {}", err_msg);
                    Err(err_msg)
                }
            }
        } else {
            // 4. 发送通道不存在
            let err_msg = format!(
                "无法发送 WebSocket 消息 (类型: '{}', ID: '{}')：发送通道不可用 (可能连接已关闭或正在关闭)。",
                message.message_type, message.message_id
            );
            warn!("[现场端移动服务] (发送消息) {}", err_msg);
            Err(err_msg)
        }
    }

     /// 获取当前本地缓存的最新任务调试状态 (`TaskDebugState`)。
     ///
     /// 此方法允许应用的其他部分（例如 Tauri 命令）查询由 WebSocket 服务维护的、
     /// 从云端同步过来的任务状态副本。
     ///
     /// # 返回
     /// * `Option<TaskDebugState>`
    pub async fn get_cached_task_state(&self) -> Option<TaskDebugState> {
        info!("[现场端移动服务] WebSocketClientService::get_cached_task_state 方法被调用，正在获取本地缓存的任务状态。");
        let cache_guard = self.local_task_state_cache.read().await;
        // 克隆 Option<TaskDebugState>。如果 Option 是 Some，则内部的 TaskDebugState 也会被克隆
        // (因为 TaskDebugState 派生了 Clone trait)。
        // 如果 Option 是 None，则克隆结果仍然是 None。
        cache_guard.clone()
    }
}