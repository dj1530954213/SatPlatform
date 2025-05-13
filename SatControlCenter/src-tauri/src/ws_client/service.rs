// SatControlCenter/src-tauri/src/ws_client/service.rs

//! WebSocket 客户端服务模块 (`service.rs`)。
//!
//! 本模块定义并实现了 `WebSocketClientService`，它是 `SatControlCenter` 应用中
//! 负责管理与云端 `SatCloudService` 之间 WebSocket 通信的核心服务。
//!
//! 主要职责包括：
//! - **连接管理**: 建立 (`connect`)、维护和断开 (`disconnect`) 与云端服务的 WebSocket 连接。
//! - **消息收发**: 封装发送 (`send_ws_message`) 和接收 (`on_message` 回调处理) 标准化的 `WsMessage`。
//! - **状态通知**: 通过 Tauri 事件系统 (`event.rs` 中定义的事件) 向前端 Angular 应用报告连接状态的变更 (`WsConnectionStatusEvent`)、接收到的特定业务响应 (如 `EchoResponseEvent`, `WsRegistrationStatusEvent`) 或状态更新 (`LocalTaskStateUpdatedEvent`)。
//! - **心跳维持**: 启动和管理一个后台任务，定期向云端发送 Ping 消息，并处理 Pong 响应，以维持连接活跃并检测可能的断开。
//! - **上下文管理**: 存储与当前调试会话相关的上下文信息，如 `group_id`, `task_id`, 以及从云端同步的权威 `TaskDebugState`。

// --- 导入依赖 ---
// **移除不再需要的导入**
// use crate::config::WsClientConfig; // WebSocket 客户端配置
use crate::event::{
    EchoResponseEventPayload, LocalTaskStateUpdatedEvent, WsConnectionStatusEvent, WsPartnerStatusEvent, WsRegistrationStatusEvent,
    ECHO_RESPONSE_EVENT, LOCAL_TASK_STATE_UPDATED_EVENT, WS_CONNECTION_STATUS_EVENT, WS_PARTNER_STATUS_EVENT, WS_REGISTRATION_STATUS_EVENT, // 确保所有事件常量都被导入
};
// **修正导入路径，直接从 common_models 导入**
use common_models::{
    ClientRole, // 客户端角色枚举
    TaskDebugState, // 共享的任务调试状态模型
    WsMessage, // 标准 WebSocket 消息结构
    // 各种 WebSocket 消息的 Payload 定义
    EchoPayload, ErrorResponsePayload, PartnerStatusPayload, PingPayload, PongPayload, RegisterResponsePayload,
    // 消息类型常量
    ECHO_MESSAGE_TYPE, ERROR_RESPONSE_MESSAGE_TYPE, PARTNER_STATUS_UPDATE_MESSAGE_TYPE,
    PING_MESSAGE_TYPE, PONG_MESSAGE_TYPE, REGISTER_RESPONSE_MESSAGE_TYPE,
    TASK_STATE_UPDATE_MESSAGE_TYPE, // 确保任务状态更新类型常量被导入
};
use chrono::{DateTime, Utc}; // 日期和时间处理
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
}; // WebSocket 流处理
use log::{debug, error, info, warn}; // 日志记录宏
// **修正 rust_websocket_utils 导入路径 **
use rust_websocket_utils::error::WsError; // 或者直接使用 WsError 如果它在顶层导出
// 假设 ClientWsStream, ClientConnection 是从 client::transport 导出
use rust_websocket_utils::client::transport::{ClientConnection, ClientWsStream};
// **移除无效的 TransportLayer 导入**
// use rust_websocket_utils::transport::TransportLayer;
use serde_json; // JSON 序列化/反序列化
use std::sync::Arc; // 原子引用计数，用于共享状态
use tauri::{AppHandle, Emitter, Manager}; // Tauri 应用句柄, Emitter 和 State 管理
use tokio::sync::{Mutex as TokioMutex, RwLock}; // Tokio 提供的异步锁
use tokio_tungstenite::{tungstenite::Message as TungsteniteMessage, MaybeTlsStream};
use url::Url; // URL 解析
use uuid::Uuid; // UUID 生成和处理
use tokio::net::TcpStream; // **导入 TcpStream 用于类型签名**
use tokio_tungstenite::WebSocketStream; // **导入 WebSocketStream 用于类型签名**

// --- 服务结构体定义 ---

/// WebSocket 客户端服务 (`WebSocketClientService`)。
///
/// 负责管理与云端 `SatCloudService` 的单一 WebSocket 连接、消息收发、
/// 状态维护和心跳检测。
/// 通过 `Arc` 包裹并注册为 Tauri `State`，以便在 Tauri 命令中安全地访问和共享。
#[derive(Debug)] // 允许打印调试信息
pub struct WebSocketClientService {
    /// WebSocket 发送通道 (`SplitSink`) 的独占访问。
    /// `Some(sink)` 表示连接已建立且发送通道可用；`None` 表示未连接或连接已关闭。
    /// 使用 `Arc<TokioMutex<...>>` 确保在多个异步任务中对发送通道的访问是互斥和安全的。
    ws_send_channel: Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,

    /// 由云端服务在连接成功后分配给本控制中心客户端的唯一标识符 (`client_id`)。
    /// `Some(uuid)` 包含该 ID；`None` 表示尚未连接或未分配 ID。
    /// 注意：此 ID 主要用于云端识别客户端会话，中心端自身通常不需要频繁使用它。
    /// 使用 `Arc<RwLock<...>>` 允许多个任务并发读取 ID，写入则需要独占访问。
    cloud_assigned_client_id: Arc<RwLock<Option<Uuid>>>,

    /// 当前客户端加入的组 ID。
    /// 在成功调用 `register_client_with_task` 并收到成功的 `RegisterResponse` 后设置。
    current_group_id: Arc<RwLock<Option<String>>>,

    /// 当前客户端关联的任务 ID。
    /// 在成功调用 `register_client_with_task` 并收到成功的 `RegisterResponse` 后设置。
    current_task_id: Arc<RwLock<Option<String>>>,

    /// 本地缓存的、从云端同步过来的权威任务调试状态。
    /// 当收到 `TaskStateUpdate` 消息时更新此状态。
    current_task_state: Arc<RwLock<Option<TaskDebugState>>>,

    /// Tauri 应用的句柄 (`AppHandle`)。
    /// 用于从服务内部与 Tauri 系统交互，最主要是通过 `Emitter` 向前端发送事件。
    /// 此句柄在服务创建时传入，并在服务内部被克隆和使用。
    app_handle: AppHandle,

    /// 当前 WebSocket 的连接状态标志。
    /// `true` 表示已连接；`false` 表示未连接。
    /// 使用 `Arc<RwLock<...>>` 进行并发安全的读写访问。
    is_connected_status: Arc<RwLock<bool>>,

    /// WebSocket 连接管理任务 (`connect_client` 和消息接收循环) 的句柄 (`JoinHandle`)。
    /// `Some(handle)` 表示连接任务正在运行；`None` 表示没有活动的连接任务。
    /// 用于在断开连接或服务销毁时能够中止或等待连接任务结束。
    /// 使用 `Arc<TokioMutex<...>>` 保护对句柄的访问。
    connection_task_handle: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,

    /// 心跳任务的句柄 (`JoinHandle`)。
    /// `Some(handle)` 表示心跳任务 (定期发送 Ping 并检查 Pong) 正在运行。
    /// 用于在连接断开或服务状态变化时管理心跳任务的生命周期 (启动、停止)。
    /// 使用 `Arc<TokioMutex<...>>` 保护对句柄的访问。
    heartbeat_task_handle: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,

    /// 最后一次成功收到云端 Pong 消息的时间戳。
    /// 用于心跳机制中检测 Pong 是否超时。
    /// `Some(DateTime<Utc>)` 记录时间；`None` 表示尚未收到过 Pong 或连接已重置。
    /// 使用 `Arc<RwLock<...>>` 进行并发安全的读写。
    last_pong_received_at: Arc<RwLock<Option<DateTime<Utc>>>>,

    // **移除对 WsClientConfig 的直接依赖，服务内部逻辑可能需要调整以适应**
    // config: Arc<WsClientConfig>,
}

// --- 服务实现 ---

impl WebSocketClientService {
    /// 创建 `WebSocketClientService` 的新实例。
    ///
    /// # Arguments
    ///
    /// * `app_handle` - Tauri 应用的句柄，用于事件发送等。
    /// // * `config` - WebSocket 客户端的配置。 (移除)
    ///
    /// # Returns
    ///
    /// 返回一个新的 `WebSocketClientService` 实例，所有内部状态都处于初始（未连接）状态。
    pub fn new(app_handle: AppHandle /* 移除 config 参数 */) -> Self {
        info!("创建新的 WebSocketClientService 实例。");
        Self {
            ws_send_channel: Arc::new(TokioMutex::new(None)),
            cloud_assigned_client_id: Arc::new(RwLock::new(None)),
            current_group_id: Arc::new(RwLock::new(None)), // 初始化新字段
            current_task_id: Arc::new(RwLock::new(None)), // 初始化新字段
            current_task_state: Arc::new(RwLock::new(None)), // 初始化新字段
            app_handle,
            is_connected_status: Arc::new(RwLock::new(false)),
            connection_task_handle: Arc::new(TokioMutex::new(None)),
            heartbeat_task_handle: Arc::new(TokioMutex::new(None)),
            last_pong_received_at: Arc::new(RwLock::new(None)),
            // config: Arc::new(config), // 移除配置存储
        }
    }

    /// 设置当前调试上下文（组ID和任务ID）。
    ///
    /// 通常在成功处理 `RegisterResponse` 消息后调用。
    ///
    /// # Arguments
    ///
    /// * `group_id` - 成功注册到的组 ID。
    /// * `task_id` - 关联的任务 ID。
    pub async fn set_current_context(&self, group_id: String, task_id: String) {
        info!(
            "设置当前调试上下文: 组ID='{}', 任务ID='{}'",
            group_id, task_id
        );
        *self.current_group_id.write().await = Some(group_id);
        *self.current_task_id.write().await = Some(task_id);
        // 注意：TaskDebugState 通常通过 TaskStateUpdate 消息设置，而不是在这里。
    }

    /// 处理 WebSocket 连接成功建立时的逻辑 (`on_open` 回调)。
    ///
    /// 负责更新内部状态、启动心跳任务，并通过 Tauri 事件通知前端。
    ///
    /// # Arguments
    ///
    /// * `app_handle` - Tauri 应用句柄。
    /// * `cloud_client_id` - 云端分配的客户端 ID（如果连接握手时提供了）。
    /// * `is_connected_status` - 连接状态标志的原子引用。
    /// * `cloud_assigned_client_id_state` - 云端分配 ID 状态的原子引用。
    /// * `ws_send_channel_clone` - 发送通道状态的原子引用（用于心跳）。
    /// * `heartbeat_task_handle_clone` - 心跳任务句柄状态的原子引用。
    /// * `last_pong_received_at_clone` - 最后 Pong 时间状态的原子引用。
    /// * `heartbeat_interval_seconds` - 心跳发送间隔（秒）。
    /// * `pong_timeout_seconds` - Pong 响应超时时间（秒）。
    // **添加心跳参数到签名**
    async fn handle_on_open(
        app_handle: &AppHandle,
        cloud_client_id: Option<Uuid>, // 改为 Option<Uuid>
        is_connected_status: &Arc<RwLock<bool>>,
        cloud_assigned_client_id_state: &Arc<RwLock<Option<Uuid>>>, // 对应修改
        ws_send_channel_clone: Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>, // 用于心跳任务
        heartbeat_task_handle_clone: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: Arc<RwLock<Option<DateTime<Utc>>>>,
        heartbeat_interval_seconds: u64, // **新增心跳间隔参数**
        pong_timeout_seconds: u64,       // **新增 Pong 超时参数**
    ) {
        info!(
            "WebSocket 连接已成功建立。云端分配的 Client ID: {:?}",
            cloud_client_id
        );

        // 1. 更新内部连接状态
        *is_connected_status.write().await = true;
        *cloud_assigned_client_id_state.write().await = cloud_client_id; // 存储云端分配的ID

        // 2. 向前端发送连接成功事件
        let event_payload = WsConnectionStatusEvent {
            connected: true,
            client_id: cloud_client_id.map(|id| id.to_string()), // 将 Uuid 转换为 String
            error_message: None,
        };
        // **使用 emit**
        if let Err(e) = app_handle.emit(WS_CONNECTION_STATUS_EVENT, event_payload) {
            error!("向前端发送 WebSocket 连接成功事件失败: {}", e);
        }

        // 3. 重置最后接收 Pong 的时间
        *last_pong_received_at_clone.write().await = Some(Utc::now());

        // 4. 启动心跳任务
        // **现在可以传入配置信息**
        Self::start_heartbeat_task_static(
            app_handle.clone(),
            ws_send_channel_clone,
            heartbeat_task_handle_clone,
            last_pong_received_at_clone.clone(),
            is_connected_status.clone(),
            cloud_assigned_client_id_state.clone(),
            heartbeat_interval_seconds, // **传递心跳间隔**
            pong_timeout_seconds, // **传递 Pong 超时**
        ).await;
        // warn!("心跳任务未启动，因为缺少配置信息。"); // 移除旧警告
    }

    /// 处理收到的 WebSocket 消息 (`on_message` 回调)。
    ///
    /// 负责解析 `WsMessage`，根据 `message_type` 分发到不同的处理逻辑，
    /// 并处理 Ping/Pong 消息。
    async fn handle_on_message(
        app_handle: &AppHandle,
        ws_msg: WsMessage,
        cloud_assigned_client_id_state: Arc<RwLock<Option<Uuid>>>,
        last_pong_received_at_clone: Arc<RwLock<Option<DateTime<Utc>>>>,
        current_task_state_cache: Arc<RwLock<Option<TaskDebugState>>>,
    ) {
        debug!("收到 WebSocket 消息: 类型='{}'", ws_msg.message_type);

        match ws_msg.message_type.as_str() {
            // 处理 Pong 消息 (心跳响应)
            PONG_MESSAGE_TYPE => {
                debug!("收到 Pong 响应。");
                *last_pong_received_at_clone.write().await = Some(Utc::now());
            }

            // 处理 Echo 响应
            ECHO_MESSAGE_TYPE => {
                match serde_json::from_str::<EchoPayload>(&ws_msg.payload) {
                    Ok(payload) => {
                        info!("收到 Echo 响应，内容: '{}'", payload.content);
                        let event_payload = EchoResponseEventPayload { content: payload.content };
                        if let Err(e) = app_handle.emit(ECHO_RESPONSE_EVENT, event_payload) {
                            error!("向前端发送 Echo 响应事件失败: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("反序列化 EchoPayload 失败: {}, 原始负载: {}", e, ws_msg.payload);
                    }
                }
            }

            // 处理注册响应
            REGISTER_RESPONSE_MESSAGE_TYPE => {
                match serde_json::from_str::<RegisterResponsePayload>(&ws_msg.payload) {
                    Ok(payload) => {
                        info!("收到注册响应: 成功={}, ClientID={:?}, 有效组ID={:?}, 消息={:?}",
                              payload.success, payload.assigned_client_id, payload.effective_group_id, payload.message);

                        let client_id = payload.assigned_client_id;
                        let effective_group_id = payload.effective_group_id.clone();

                        // 更新内部状态 (如果注册成功，只更新 client_id)
                        if payload.success {
                            *cloud_assigned_client_id_state.write().await = Some(client_id);
                        }

                        // 向前端发送注册状态事件
                        let event_payload = WsRegistrationStatusEvent {
                            success: payload.success,
                            message: payload.message,
                            group_id: effective_group_id.unwrap_or_default(),
                            assigned_client_id: Some(client_id),
                        };
                        if let Err(e) = app_handle.emit(WS_REGISTRATION_STATUS_EVENT, event_payload) {
                            error!("向前端发送注册状态事件失败: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("反序列化 RegisterResponsePayload 失败: {}, 原始负载: {}", e, ws_msg.payload);
                    }
                }
            }

            // 处理伙伴状态更新
            PARTNER_STATUS_UPDATE_MESSAGE_TYPE => {
                match serde_json::from_str::<PartnerStatusPayload>(&ws_msg.payload) {
                    Ok(payload) => {
                        info!("收到伙伴状态更新: 角色={:?}, ClientID={}, 在线={}, 组ID={}",
                              payload.partner_role, payload.partner_client_id, payload.is_online, payload.group_id);

                        // 向前端发送伙伴状态事件
                        let event_payload = WsPartnerStatusEvent {
                            partner_role: payload.partner_role,
                            partner_client_id: payload.partner_client_id,
                            is_online: payload.is_online,
                            group_id: payload.group_id,
                        };
                        if let Err(e) = app_handle.emit(WS_PARTNER_STATUS_EVENT, event_payload) {
                            error!("向前端发送伙伴状态事件失败: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("反序列化 PartnerStatusPayload 失败: {}, 原始负载: {}", e, ws_msg.payload);
                    }
                }
            }

            // 处理任务状态更新
            TASK_STATE_UPDATE_MESSAGE_TYPE => {
                match serde_json::from_str::<TaskDebugState>(&ws_msg.payload) {
                    Ok(new_state) => {
                        info!("收到任务状态更新 (TaskStateUpdate)，任务 ID: {}", new_state.task_id);
                        debug!("新任务状态详情: {:?}", new_state); // 打印完整状态 (Debug)

                        // 更新本地缓存的任务状态
                        *current_task_state_cache.write().await = Some(new_state.clone());

                        // 向前端发送本地任务状态已更新事件
                        let event_payload = LocalTaskStateUpdatedEvent { new_state };
                        if let Err(e) = app_handle.emit(LOCAL_TASK_STATE_UPDATED_EVENT, event_payload) {
                            error!("向前端发送本地任务状态更新事件失败: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("反序列化 TaskDebugState 失败: {}, 原始负载: {}", e, ws_msg.payload);
                    }
                }
            }

            // 处理错误响应
            ERROR_RESPONSE_MESSAGE_TYPE => {
                match serde_json::from_str::<ErrorResponsePayload>(&ws_msg.payload) {
                    Ok(payload) => {
                        error!("收到来自云端的错误响应: {}, 原始消息类型: {:?}",
                               payload.error, payload.original_message_type);
                        // 可选：向前端发送一个特定的错误事件
                        // let event_payload = WsServerErrorEvent { ... };
                        // app_handle.emit(WS_SERVER_ERROR_EVENT, event_payload).ok();
                    }
                    Err(e) => {
                        error!("反序列化 ErrorResponsePayload 失败: {}, 原始负载: {}", e, ws_msg.payload);
                    }
                }
            }

            // 处理其他未知或未实现的消息类型
            _ => {
                warn!("收到未知或未处理的 WebSocket 消息类型: '{}'", ws_msg.message_type);
            }
        }
    }

    /// 处理 WebSocket 连接关闭时的逻辑 (`on_close` 回调)。
    ///
    /// 负责清理状态、停止心跳任务，并通过 Tauri 事件通知前端。
    async fn handle_on_close(
        app_handle: &AppHandle,
        reason: Option<String>,
        is_connected_status: &Arc<RwLock<bool>>,
        cloud_assigned_client_id_state: &Arc<RwLock<Option<Uuid>>>,
        ws_send_channel_clone_for_cleanup: &Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        heartbeat_task_handle_clone: &Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
        current_group_id_state: &Arc<RwLock<Option<String>>>,
        current_task_id_state: &Arc<RwLock<Option<String>>>,
        current_task_state_cache: &Arc<RwLock<Option<TaskDebugState>>>,
    ) {
        info!("WebSocket 连接已关闭。原因: {:?}", reason);

        // 1. 停止心跳任务
        Self::stop_heartbeat_task_static(
            heartbeat_task_handle_clone,
            last_pong_received_at_clone,
        )
        .await;

        // 2. 清理内部状态
        *is_connected_status.write().await = false;
        let old_client_id = cloud_assigned_client_id_state.write().await.take(); // 清理并获取旧 ID
        *ws_send_channel_clone_for_cleanup.lock().await = None; // 清理发送通道
        // 正确清理 RwLock<Option<T>>
        *current_group_id_state.write().await = None;
        *current_task_id_state.write().await = None;
        *current_task_state_cache.write().await = None;

        // 3. 向前端发送连接关闭事件
        let event_payload = WsConnectionStatusEvent {
            connected: false,
            client_id: old_client_id.map(|id| id.to_string()), // 发送关闭前的 ID
            error_message: reason.or_else(|| Some("连接已关闭 (无特定原因)".to_string())),
        };
        // **使用 emit**
        if let Err(e) = app_handle.emit(WS_CONNECTION_STATUS_EVENT, event_payload) {
            error!("向前端发送 WebSocket 连接关闭事件失败: {}", e);
        }
    }

    /// 处理 WebSocket 连接过程中发生的错误 (`on_error` 回调)。
    ///
    /// 负责记录错误、清理状态、停止心跳，并通过 Tauri 事件通知前端。
    async fn handle_on_error(
        app_handle: &AppHandle,
        error_message_str: String,
        is_connected_status: &Arc<RwLock<bool>>,
        cloud_assigned_client_id_state: &Arc<RwLock<Option<Uuid>>>,
        ws_send_channel_clone_for_cleanup: &Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        heartbeat_task_handle_clone: &Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
        current_group_id_state: &Arc<RwLock<Option<String>>>,
        current_task_id_state: &Arc<RwLock<Option<String>>>,
        current_task_state_cache: &Arc<RwLock<Option<TaskDebugState>>>,
    ) {
        error!("WebSocket 连接发生错误: {}", error_message_str);

        // 1. 停止心跳任务 (如果正在运行)
        Self::stop_heartbeat_task_static(
            heartbeat_task_handle_clone,
            last_pong_received_at_clone,
        )
        .await;

        // 2. 清理内部状态 (与 handle_on_close 类似)
        let was_connected = *is_connected_status.read().await;
        if was_connected { // 仅当之前是连接状态时才清理
            *is_connected_status.write().await = false;
            let old_client_id = cloud_assigned_client_id_state.write().await.take();
            *ws_send_channel_clone_for_cleanup.lock().await = None;
            // 正确清理 RwLock<Option<T>>
            *current_group_id_state.write().await = None;
            *current_task_id_state.write().await = None;
            *current_task_state_cache.write().await = None;

            // 3. 向前端发送连接错误/断开事件
            let event_payload = WsConnectionStatusEvent {
                connected: false,
                client_id: old_client_id.map(|id| id.to_string()),
                error_message: Some(error_message_str.clone()), // 克隆错误消息
            };
            // **使用 emit**
            if let Err(e) = app_handle.emit(WS_CONNECTION_STATUS_EVENT, event_payload) {
                error!("向前端发送 WebSocket 连接错误事件失败: {}", e);
            }
        } else {
             // 如果之前就不是连接状态 (例如，连接尝试失败)，只发送错误事件
             let event_payload = WsConnectionStatusEvent {
                connected: false,
                client_id: None, // 连接失败，没有 client_id
                error_message: Some(error_message_str), // 使用原始错误消息
            };
            // **使用 emit**
            if let Err(e) = app_handle.emit(WS_CONNECTION_STATUS_EVENT, event_payload) {
                error!("向前端发送 WebSocket 连接尝试失败事件失败: {}", e);
            }
        }
    }

    /// 启动心跳任务 (静态方法版本，便于在回调中调用)。
    ///
    /// 定期发送 Ping 消息，并检查 Pong 响应是否超时。
    ///
    /// **注意：需要传入配置参数。**
    async fn start_heartbeat_task_static(
        app_handle: AppHandle,
        ws_send_channel_clone: Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        heartbeat_task_handle_clone: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: Arc<RwLock<Option<DateTime<Utc>>>>,
        is_connected_status_clone: Arc<RwLock<bool>>,
        cloud_assigned_client_id_clone: Arc<RwLock<Option<Uuid>>>, // Uuid
        heartbeat_interval_seconds: u64, // **传入心跳间隔**
        pong_timeout_seconds: u64, // **传入 Pong 超时**
    ) {
        let mut handle_guard = heartbeat_task_handle_clone.lock().await;
        if handle_guard.is_some() {
            info!("心跳任务已在运行。");
            return;
        }

        info!("启动 WebSocket 心跳任务 (间隔: {}秒, Pong 超时: {}秒)", heartbeat_interval_seconds, pong_timeout_seconds);

        // 启动一个新的 Tokio 任务来执行心跳逻辑
        let heartbeat_task = tokio::spawn(async move {
            let ping_interval = tokio::time::Duration::from_secs(heartbeat_interval_seconds);
            let pong_timeout = tokio::time::Duration::from_secs(pong_timeout_seconds);
            let mut interval = tokio::time::interval(ping_interval);

            loop {
                interval.tick().await; // 等待下一个心跳周期

                // 检查连接状态，如果已断开则退出心跳任务
                if !*is_connected_status_clone.read().await {
                    info!("连接已断开，停止心跳任务。");
                    break;
                }

                // 检查 Pong 是否超时
                let last_pong_time = *last_pong_received_at_clone.read().await;
                if let Some(last_pong) = last_pong_time {
                    if Utc::now().signed_duration_since(last_pong).to_std().unwrap_or_default() > pong_timeout {
                        warn!("Pong 响应超时 (超过 {} 秒未收到)，可能连接已丢失。", pong_timeout_seconds);
                        // 触发连接错误处理 (模拟错误，让主连接循环处理断开)
                        let error_msg = "Pong timeout".to_string();
                        // 需要一个方法来从这里触发 handle_on_error 或类似逻辑
                        // 暂时只记录警告，依赖主循环检测断开
                        // TODO: 实现更主动的断开处理机制
                        break; // 退出心跳任务
                    }
                }

                // 发送 Ping 消息
                let ping_payload = PingPayload {};
                match serde_json::to_string(&ping_payload) {
                    Ok(payload_json) => {
                        let ping_msg = WsMessage {
                            message_id: Uuid::new_v4().to_string(),
                            timestamp: Utc::now().timestamp_millis(),
                            message_type: PING_MESSAGE_TYPE.to_string(),
                            payload: payload_json,
                        };
                        match serde_json::to_string(&ping_msg) {
                            Ok(msg_str) => {
                                let tungstenite_msg = TungsteniteMessage::Text(msg_str);
                                let mut sender_guard = ws_send_channel_clone.lock().await;
                                if let Some(sender) = sender_guard.as_mut() {
                                    if let Err(e) = sender.send(tungstenite_msg).await {
                                        error!("发送 Ping 消息失败: {}", e);
                                        // 发送失败可能意味着连接已断开，退出心跳任务
                                        break;
                                    } else {
                                        debug!("Ping 消息已发送。");
                                    }
                                } else {
                                    info!("发送通道不可用，无法发送 Ping，停止心跳任务。");
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("序列化 Ping WsMessage 失败: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("序列化 PingPayload 失败: {}", e);
                    }
                }
            }
            info!("心跳任务已退出。");
        });

        // 存储心跳任务的句柄
        *handle_guard = Some(heartbeat_task);
    }

    /// 停止心跳任务 (静态方法版本)。
    async fn stop_heartbeat_task_static(
        heartbeat_task_handle_clone: &Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
    ) {
        let mut handle_guard = heartbeat_task_handle_clone.lock().await;
        if let Some(handle) = handle_guard.take() { // take() 会移除句柄
            info!("正在停止 WebSocket 心跳任务...");
            handle.abort(); // 请求中止任务
            match handle.await {
                Ok(_) => info!("心跳任务已成功中止。"),
                Err(e) if e.is_cancelled() => info!("心跳任务已成功中止 (cancelled)。"),
                Err(e) => warn!("等待心跳任务中止时发生错误: {:?}", e),
            }
        } else {
            debug!("没有正在运行的心跳任务需要停止。");
        }
        // 清理最后 Pong 时间
        *last_pong_received_at_clone.write().await = None;
    }

    /// 尝试连接到指定的 WebSocket URL。
    ///
    /// 这是供 Tauri 命令 (`connect_to_cloud`) 调用的主要入口点。
    pub async fn connect(&self, url_str: &str) -> Result<(), String> {
        info!("WebSocket 服务收到连接请求，目标 URL: {}", url_str);

        // 检查是否已连接或正在连接
        if *self.is_connected_status.read().await {
            let msg = "WebSocket 已连接。无需重复连接。".to_string();
            info!("{}", msg);
            return Ok(()); // 幂等操作，直接返回成功
        }
        if self.connection_task_handle.lock().await.is_some() {
            let msg = "WebSocket 正在连接中... 请稍候。".to_string();
            warn!("{}", msg);
            return Err(msg);
        }

        // 解析 URL
        let url = match Url::parse(url_str) {
            Ok(u) => u,
            Err(e) => {
                let err_msg = format!("无效的 WebSocket URL '{}': {}", url_str, e);
                error!("{}", err_msg);
                // 在启动任务失败时也需要通知前端 (此逻辑已在 handle_on_error 中)
                // 因此，直接返回错误即可
                return Err(err_msg);
            }
        };

        // 克隆状态
        let app_handle_clone = self.app_handle.clone();
        let ws_send_channel_clone = self.ws_send_channel.clone();
        let is_connected_status_clone = self.is_connected_status.clone();
        let cloud_assigned_client_id_clone = self.cloud_assigned_client_id.clone();
        let heartbeat_task_handle_clone = self.heartbeat_task_handle.clone();
        let last_pong_received_at_clone = self.last_pong_received_at.clone();
        let connection_task_handle_clone = self.connection_task_handle.clone();
        let current_group_id_clone = self.current_group_id.clone();
        let current_task_id_clone = self.current_task_id.clone();
        let current_task_state_clone = self.current_task_state.clone();

        // 启动连接任务
        let connection_task = tokio::spawn(async move {
            info!("启动 WebSocket 连接任务...");

            match rust_websocket_utils::client::transport::connect_client(
                url.to_string(),
            )
            .await
            {
                Ok(client_connection) => {
                    let ws_sender = client_connection.ws_sender;
                    let ws_receiver = client_connection.ws_receiver;
                    *ws_send_channel_clone.lock().await = Some(ws_sender);

                    let heartbeat_interval = 30;
                    let pong_timeout = 10;
                    let maybe_client_id = None;

                    Self::handle_on_open(
                        &app_handle_clone,
                        maybe_client_id,
                        &is_connected_status_clone,
                        &cloud_assigned_client_id_clone,
                        ws_send_channel_clone.clone(),
                        heartbeat_task_handle_clone.clone(),
                        last_pong_received_at_clone.clone(),
                        heartbeat_interval,
                        pong_timeout,
                    )
                    .await;

                    Self::message_receive_loop(
                        ws_receiver,
                        &app_handle_clone,
                        cloud_assigned_client_id_clone.clone(),
                        last_pong_received_at_clone.clone(),
                        current_task_state_clone.clone(),
                        &is_connected_status_clone,
                        &ws_send_channel_clone,
                        &heartbeat_task_handle_clone,
                        &current_group_id_clone,
                        &current_task_id_clone,
                    )
                    .await;
                }
                Err(e) => {
                    let err_msg = format!("连接到 WebSocket 服务器 '{}' 失败: {}", url, e);
                    error!("{}", err_msg);
                    Self::handle_on_error(
                        &app_handle_clone,
                        err_msg,
                        &is_connected_status_clone,
                        &cloud_assigned_client_id_clone,
                        &ws_send_channel_clone,
                        &heartbeat_task_handle_clone,
                        &last_pong_received_at_clone,
                        &current_group_id_clone,
                        &current_task_id_clone,
                        &current_task_state_clone,
                    )
                    .await;
                }
            }
            info!("WebSocket 连接任务结束。");
            *connection_task_handle_clone.lock().await = None;
        });

        *self.connection_task_handle.lock().await = Some(connection_task);

        Ok(())
    }

    /// WebSocket 消息接收循环。
    /// **修改 Stream 的 Item 类型为 Result<..., tungstenite::Error>**
    async fn message_receive_loop(
        mut stream: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>, // **使用具体类型而非泛型**
        app_handle: &AppHandle,
        cloud_assigned_client_id_state: Arc<RwLock<Option<Uuid>>>,
        last_pong_received_at_clone: Arc<RwLock<Option<DateTime<Utc>>>>,
        current_task_state_cache: Arc<RwLock<Option<TaskDebugState>>>,
        is_connected_status_for_cleanup: &Arc<RwLock<bool>>,
        ws_send_channel_for_cleanup: &Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>, // **类型需确认**
        heartbeat_task_handle_for_cleanup: &Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        current_group_id_for_cleanup: &Arc<RwLock<Option<String>>>,
        current_task_id_for_cleanup: &Arc<RwLock<Option<String>>>,
    ) {
        info!("启动 WebSocket 消息接收循环...");
        while let Some(message_result) = stream.next().await {
            match message_result {
                Ok(msg) => {
                    match msg {
                        TungsteniteMessage::Text(text) => {
                            match serde_json::from_str::<WsMessage>(&text) {
                                Ok(ws_msg) => {
                                    Self::handle_on_message(
                                        app_handle,
                                        ws_msg,
                                        cloud_assigned_client_id_state.clone(),
                                        last_pong_received_at_clone.clone(),
                                        current_task_state_cache.clone(),
                                    )
                                    .await;
                                }
                                Err(e) => {
                                    error!("反序列化收到的文本消息为 WsMessage 失败: {}, 原始文本: {}", e, text);
                                }
                            }
                        }
                        TungsteniteMessage::Binary(bin) => {
                            warn!("收到未处理的二进制 WebSocket 消息，长度: {} bytes", bin.len());
                        }
                        TungsteniteMessage::Ping(_) => {
                            debug!("收到 Ping 消息，自动由 tungstenite 回复 Pong。");
                            *last_pong_received_at_clone.write().await = Some(Utc::now());
                        }
                        TungsteniteMessage::Pong(_) => {
                            debug!("明确收到 Pong 消息");
                            *last_pong_received_at_clone.write().await = Some(Utc::now());
                        }
                        TungsteniteMessage::Close(close_frame) => {
                            info!("收到 Close 帧，WebSocket 连接正在关闭。帧信息: {:?}", close_frame);
                            let reason = close_frame.map(|cf| format!("Code: {}, Reason: {}", cf.code, cf.reason));
                            Self::handle_on_close(
                                app_handle,
                                reason,
                                is_connected_status_for_cleanup,
                                &cloud_assigned_client_id_state,
                                ws_send_channel_for_cleanup,
                                heartbeat_task_handle_for_cleanup,
                                &last_pong_received_at_clone,
                                current_group_id_for_cleanup,
                                current_task_id_for_cleanup,
                                &current_task_state_cache,
                            ).await;
                            break;
                        }
                        TungsteniteMessage::Frame(_) => {
                            debug!("收到原始 WebSocket Frame，忽略。");
                        }
                    }
                }
                Err(e) => {
                    let error_message_str = format!("WebSocket 接收流错误: {}", e);
                    error!("{}", error_message_str);
                    Self::handle_on_error(
                        app_handle,
                        error_message_str,
                        is_connected_status_for_cleanup,
                        &cloud_assigned_client_id_state,
                        ws_send_channel_for_cleanup,
                        heartbeat_task_handle_for_cleanup,
                        &last_pong_received_at_clone,
                        current_group_id_for_cleanup,
                        current_task_id_for_cleanup,
                        &current_task_state_cache,
                    ).await;
                    break;
                }
            }
        }
        info!("WebSocket 消息接收循环结束。");
    }

    /// 断开当前的 WebSocket 连接。
    ///
    /// 供 Tauri 命令 (`disconnect_from_cloud`) 调用。
    pub async fn disconnect(&self) -> Result<(), String> {
        info!("WebSocket 服务收到断开连接请求。");

        // 停止心跳任务
        Self::stop_heartbeat_task_static(
            &self.heartbeat_task_handle,
            &self.last_pong_received_at,
        )
        .await;

        // 中止连接任务 (如果有)
        let mut conn_handle_guard = self.connection_task_handle.lock().await;
        if let Some(handle) = conn_handle_guard.take() {
            info!("正在中止 WebSocket 连接任务...");
            handle.abort();
            match handle.await {
                Ok(_) => info!("连接任务已成功中止。"),
                Err(e) if e.is_cancelled() => info!("连接任务已成功中止 (cancelled)。"),
                Err(e) => warn!("等待连接任务中止时发生错误: {:?}", e),
            }
        } else {
            debug!("没有活动的连接任务需要中止。");
        }

        // 清理状态变量 (已在 handle_on_close/error 中处理，但这里可以作为保险)
        let was_connected = *self.is_connected_status.read().await;
        if was_connected {
            *self.is_connected_status.write().await = false;
             let old_client_id = self.cloud_assigned_client_id.write().await.take();
             *self.ws_send_channel.lock().await = None; // 确保清理
             *self.current_group_id.write().await = None;
             *self.current_task_id.write().await = None;
             *self.current_task_state.write().await = None;

            // 发送连接关闭事件给前端
            let event_payload = WsConnectionStatusEvent {
                connected: false,
                client_id: old_client_id.map(|id| id.to_string()),
                error_message: Some("用户请求断开连接".to_string()),
            };
            // **使用 emit**
            if let Err(e) = self.app_handle.emit(WS_CONNECTION_STATUS_EVENT, event_payload) {
                error!("向前端发送 WebSocket 用户断开事件失败: {}", e);
            }
            info!("WebSocket 连接已根据用户请求断开。");
        } else {
            info!("WebSocket 本身未连接，无需断开。");
        }

        Ok(())
    }

    /// 检查当前 WebSocket 是否已连接。
    pub async fn is_connected(&self) -> bool {
        *self.is_connected_status.read().await
    }

    /// 向 WebSocket 发送一个 `WsMessage`。
    ///
    /// 供 Tauri 命令 (`send_ws_echo`, `register_client_with_task` 等) 调用。
    /// 负责将 `WsMessage` 序列化为 JSON 字符串，并将其作为 `Text` 消息发送出去。
    pub async fn send_ws_message(&self, message: WsMessage) -> Result<(), String> {
        debug!(
            "尝试发送 WebSocket 消息: 类型='{}', ID={}",
            message.message_type,
            message.message_id
        );

        // 检查连接状态
        if !self.is_connected().await {
            let err_msg = format!(
                "无法发送类型为 '{}' 的消息：WebSocket 未连接。",
                message.message_type
            );
            warn!("{}", err_msg);
            return Err(err_msg);
        }

        // 序列化 WsMessage 为 JSON 字符串
        let message_str = match serde_json::to_string(&message) {
            Ok(s) => s,
            Err(e) => {
                let err_msg = format!(
                    "序列化类型为 '{}' 的 WsMessage 失败: {}",
                    message.message_type, e
                );
                error!("{}", err_msg);
                return Err(err_msg);
            }
        };

        // 获取发送通道的锁
        let mut sender_guard = self.ws_send_channel.lock().await;

        // 检查发送通道是否有效
        if let Some(sender) = sender_guard.as_mut() {
            // 发送 Text 消息
            match sender.send(TungsteniteMessage::Text(message_str)).await {
                Ok(_) => {
                    debug!(
                        "类型为 '{}' 的消息已成功提交到发送队列。",
                        message.message_type
                    );
                    Ok(())
                }
                Err(e) => {
                    let err_msg = format!(
                        "发送类型为 '{}' 的消息失败: {}",
                        message.message_type, e
                    );
                    error!("{}", err_msg);
                    // 发送失败可能意味着连接已断开，需要处理状态
                    // TODO: 考虑在这里主动调用 disconnect 或 handle_on_error
                    Err(err_msg)
                }
            }
        } else {
            // 如果没有发送通道
            let err_msg = format!(
                "无法发送类型为 '{}' 的消息：发送通道不可用。",
                message.message_type
            );
            error!("{}", err_msg);
            // 在这里也应该考虑清理状态
            Err(err_msg)
        }
    }

     // 辅助方法：克隆服务状态，用于在异步回调中捕获 self
     // **移除 Clone 方法，因为 WebSocketClientService 不容易实现 Clone**
     // fn clone_service_state(&self) -> Self { ... }
}

// 实现 Drop trait 以确保在服务实例被销毁时进行清理
// impl Drop for WebSocketClientService {
//     fn drop(&mut self) {
//         info!("WebSocketClientService 即将被 Drop，执行清理操作...");
//         // 注意：drop 是同步执行的，不能直接调用 async 方法
//         // 需要在运行时显式调用异步的 disconnect 方法来正确清理
//         // 或者使用 tokio::runtime::Handle::block_on 来运行异步清理代码（但不推荐在 drop 中这样做）
//         if self.connection_task_handle.try_lock().unwrap().is_some() || self.heartbeat_task_handle.try_lock().unwrap().is_some() {
//              warn!("WebSocketClientService 被 Drop 时仍有活动的后台任务！可能导致资源泄露。请确保在销毁前调用 disconnect()。");
//         }
//         // 理论上，Arc 包裹的状态会在最后一个 Arc 引用消失时自动释放
//         info!("WebSocketClientService Drop 完成。");
//     }
// }