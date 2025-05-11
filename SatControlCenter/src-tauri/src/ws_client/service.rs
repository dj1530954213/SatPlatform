// SatControlCenter/src-tauri/src/ws_client/service.rs

//! WebSocket 客户端服务模块，核心职责是管理与云端 WebSocket 服务的连接、消息交互和状态维护。
//!
//! 本模块定义了 `WebSocketClientService` 结构体，它封装了所有与云端 `SatCloudService`
//! 进行 WebSocket 通信所需的核心逻辑。这包括建立连接、处理传入和传出消息、
//! 实现心跳机制以保持连接活跃、以及在连接状态发生变化时通知应用的其他部分 (例如通过 Tauri 事件)。

use anyhow::Result; // anyhow 用于更灵活的错误处理 (尽管在此模块中错误主要通过 String 返回给 Tauri 命令调用方)
use log::{debug, error, info, warn}; // `log` crate，用于记录不同级别的日志信息。
use rust_websocket_utils::client::transport::{connect_client, ClientConnection, receive_message, ClientWsStream}; // 从 `rust_websocket_utils` 导入客户端连接和消息接收工具。
use rust_websocket_utils::message::WsMessage; // 标准的 WebSocket 消息结构体。
use tauri::{AppHandle, Emitter}; // `AppHandle` 用于访问 Tauri 应用实例，`Emitter` 用于发送事件。
use tokio::sync::{RwLock}; // `RwLock` (读写锁)，用于并发安全的内部状态读写。
use tokio::sync::Mutex as TokioMutex; // `TokioMutex` (Tokio 的互斥锁)，用于保护共享资源的独占访问。
// use url::Url; // 已移除：未使用的 Url crate 导入。
use std::sync::Arc; // `Arc` (原子引用计数)，用于在多线程/异步任务间安全地共享状态所有权。
use futures_util::stream::{SplitSink}; // `SplitSink`，代表 WebSocket 连接的发送半部分。
use tokio_tungstenite::tungstenite::protocol::Message as TungsteniteMessage; // 底层的 Tungstenite WebSocket 消息枚举。
use futures_util::SinkExt; // 为 `SplitSink` 提供 `send` 等扩展方法。
use chrono::{Utc, DateTime}; // `chrono` crate，用于处理日期和时间，特别是心跳机制中的时间戳。
use std::time::Duration; // `Duration`，用于定义时间间隔，例如心跳的周期。

// 从当前 crate 的 `event` 模块导入事件常量和事件负载结构体。
use crate::event::{WS_CONNECTION_STATUS_EVENT, WsConnectionStatusEvent, ECHO_RESPONSE_EVENT, EchoResponseEventPayload};
// 从 `common_models` crate 导入共享数据模型，特别是 WebSocket 负载定义中的消息类型常量和结构体。
use common_models::{self, ws_payloads::{PING_MESSAGE_TYPE, PONG_MESSAGE_TYPE, PingPayload}};

// --- 心跳机制相关常量定义 ---
/// 心跳发送间隔，单位为秒。
/// 控制中心将每隔 `HEARTBEAT_INTERVAL_SECONDS` 秒向云端发送一个 Ping 消息。
const HEARTBEAT_INTERVAL_SECONDS: u64 = 30;
/// Pong 消息接收超时时间，单位为秒。
/// 在发送 Ping 消息后，控制中心期望在 `PONG_TIMEOUT_SECONDS` 秒内收到云端的 Pong 回复。
/// 如果超过 `HEARTBEAT_INTERVAL_SECONDS + PONG_TIMEOUT_SECONDS` (总等待窗口)仍未收到 Pong，
/// 则认为连接可能已丢失，并会触发断开连接和错误处理逻辑。
const PONG_TIMEOUT_SECONDS: u64 = 10;

/// WebSocket 客户端服务 (`WebSocketClientService`) 结构体定义。
///
/// 本结构体是 `SatControlCenter` 与云端 `SatCloudService` 进行 WebSocket 通信的核心。
/// 它负责管理连接的整个生命周期，包括：
/// - 建立和维护 WebSocket 连接。
/// - 异步发送消息到云端。
/// - 异步接收并处理来自云端的消息。
/// - 实现心跳机制 (Ping/Pong) 以保持连接活跃并检测断开。
/// - 维护和更新连接状态 (例如，是否已连接，云端分配的客户端ID)。
/// - 通过 Tauri 事件系统将连接状态变化和重要消息通知给前端 UI。
///
/// 结构体的字段大多使用 `Arc` 和并发原语 (如 `TokioMutex`, `RwLock`) 包装，
/// 以便服务实例本身可以被 `Arc` 包裹后安全地在 Tauri 的托管状态中共享，
/// 并被多个异步任务 (如连接任务、心跳任务、Tauri 命令)并发访问。
#[derive(Debug)] // 派生 Debug trait，方便在调试时打印服务实例的信息。
pub struct WebSocketClientService {
    /// WebSocket 发送通道 (`SplitSink`) 的独占访问。
    /// `Some(sink)` 表示连接已建立且发送通道可用；`None` 表示未连接或连接已关闭。
    /// 使用 `Arc<TokioMutex<...>>` 确保在多个异步任务中对发送通道的访问是互斥和安全的。
    ws_send_channel: Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,

    /// 由云端服务在连接成功后分配给本控制中心客户端的唯一标识符 (`client_id`)。
    /// `Some(String)` 包含该 ID；`None` 表示尚未连接或未分配 ID。
    /// 使用 `Arc<RwLock<...>>` 允许多个任务并发读取 ID，写入则需要独占访问。
    cloud_assigned_client_id: Arc<RwLock<Option<String>>>,

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
}

impl WebSocketClientService {
    /// 创建一个新的 `WebSocketClientService` (WebSocket客户端服务) 实例。
    ///
    /// # 参数
    /// - `app_handle`: `AppHandle` - 一个 Tauri 应用句柄的克隆。服务将使用此句柄
    ///   向前端发送事件 (例如，连接状态更新、收到的消息等)。
    ///
    /// # 返回值
    /// - `Self`: 一个新创建的 `WebSocketClientService` 实例，其所有内部状态都被初始化为
    ///   表示"未连接"的默认值。
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            ws_send_channel: Arc::new(TokioMutex::new(None)), // 发送通道初始为 None
            cloud_assigned_client_id: Arc::new(RwLock::new(None)), // 云端客户端ID初始为 None
            app_handle, // 存储传入的 AppHandle
            is_connected_status: Arc::new(RwLock::new(false)), // 连接状态初始为 false (未连接)
            connection_task_handle: Arc::new(TokioMutex::new(None)), // 连接任务句柄初始为 None
            heartbeat_task_handle: Arc::new(TokioMutex::new(None)), // 心跳任务句柄初始为 None
            last_pong_received_at: Arc::new(RwLock::new(None)), // 最后收到Pong的时间初始为 None
        }
    }

    // --- 静态版本的连接事件回调辅助函数 ---
    // 这些回调函数被设计为静态方法 (不接收 `&self` 或 `&mut self`)，因为它们通常在
    // `tokio::spawn` 创建的独立异步任务中被调用。这些任务可能拥有服务的部分状态 (通过 `Arc` 克隆)
    // 而不是服务实例本身。因此，这些函数通过参数接收所需的 `AppHandle` (应用句柄)
    // 以及对相关共享状态 (`Arc<Mutex/RwLock<...>>`) 的引用。

    /// WebSocket 连接成功打开时的静态回调处理函数。
    ///
    /// 当 `connect_client` 成功建立 WebSocket 连接后，此函数被调用。
    /// 主要职责包括：
    /// - 更新内部连接状态 (`is_connected_status`, `cloud_assigned_client_id_state`)。
    /// - 初始化 `last_pong_received_at` 以启动心跳超时检测。
    /// - 向前端发送 `WS_CONNECTION_STATUS_EVENT` 事件，通知连接成功。
    /// - 启动心跳任务 (`start_heartbeat_task_static`)。
    ///
    /// # 参数
    /// - `app_handle`: `&AppHandle` - Tauri 应用句柄的引用，用于发送事件。
    /// - `cloud_client_id`: `Option<String>` - 云端可能在连接握手时分配的客户端 ID。
    /// - `is_connected_status`: `&Arc<RwLock<bool>>` - 对连接状态标志的共享引用。
    /// - `cloud_assigned_client_id_state`: `&Arc<RwLock<Option<String>>>` - 对云端客户端ID状态的共享引用。
    /// - `ws_send_channel_clone`: `Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>` - 对发送通道的共享引用，传递给心跳任务。
    /// - `heartbeat_task_handle_clone`: `Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>` - 对心跳任务句柄的共享引用。
    /// - `last_pong_received_at_clone`: `Arc<RwLock<Option<DateTime<Utc>>>>` - 对最后Pong时间的共享引用。
    async fn handle_on_open(
        app_handle: &AppHandle,
        cloud_client_id: Option<String>,
        is_connected_status: &Arc<RwLock<bool>>,
        cloud_assigned_client_id_state: &Arc<RwLock<Option<String>>>,
        ws_send_channel_clone: Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        heartbeat_task_handle_clone: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: Arc<RwLock<Option<DateTime<Utc>>>>,
    ) {
        // 这是一个非常独特的日志标记，用于在复杂的日志流中快速定位到此函数的执行点，尤其是在调试涉及多个组件和异步流程的场景。
        // 保留此标记有助于问题追踪，特别是当怀疑此回调是否按预期被调用时。
        log::error!("SAT_CONTROL_CENTER_WS_CLIENT_HANDLE_ON_OPEN_CALLBACK_V1_MARKER"); // 独特的错误级别日志标记，便于追踪
        info!("（静态回调）WebSocket 连接已成功打开。云端客户端ID (若此时已知): {:?}", cloud_client_id);
        
        // 更新共享状态：标记为已连接，并存储云端分配的客户端ID (如果存在)
        *is_connected_status.write().await = true;
        *cloud_assigned_client_id_state.write().await = cloud_client_id.clone();
        // 初始化最后收到 Pong 的时间为当前时间，为心跳超时检测做准备
        *last_pong_received_at_clone.write().await = Some(Utc::now());

        // 构建连接成功事件的负载
        let event_payload = WsConnectionStatusEvent {
            connected: true,
            error_message: Some("已成功连接到云端服务。".to_string()), // 提供更友好的中文消息
            client_id: cloud_client_id, // 传递云端分配的客户端ID
        };
        // 尝试向前端（主窗口）发送连接状态更新事件
        if let Err(e) = app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, &event_payload) {
            error!("（静态回调）发送 '已连接' 事件到前端失败: {}", e);
        }

        info!("（handle_on_open 静态回调）即将调用 start_heartbeat_task_static (启动静态心跳任务) 函数...");

        // 启动心跳任务。注意这里传递的是克隆后的 Arc 引用。
        Self::start_heartbeat_task_static(
            app_handle.clone(), // 克隆 AppHandle 传递给新任务
            ws_send_channel_clone, // 传递发送通道的 Arc 引用
            heartbeat_task_handle_clone, // 传递心跳任务句柄的 Arc 引用
            last_pong_received_at_clone, // 传递最后Pong时间的 Arc 引用
            is_connected_status.clone(), // 克隆连接状态的 Arc 引用
            cloud_assigned_client_id_state.clone(), // 克隆客户端ID状态的 Arc 引用
        ).await;

        info!("（handle_on_open 静态回调）start_heartbeat_task_static (启动静态心跳任务) 函数调用完成。");
    }

    /// 从云端成功接收到 WebSocket 消息时的静态回调处理函数。
    ///
    /// 当 `receive_message` 协程从 WebSocket 流中读取到一个完整的 `WsMessage` 后，此函数被调用。
    /// 主要职责包括：
    /// - 记录收到的消息详情 (类型和负载的摘要)。
    /// - 根据 `ws_msg.message_type` (消息类型) 进行分发处理：
    ///   - 如果是 `ECHO_MESSAGE_TYPE` (回声消息)，则解析其负载 (`EchoPayload`)，
    ///     并向前端发送 `ECHO_RESPONSE_EVENT` 事件，将回声内容传递回去。
    ///   - 如果是 `PONG_MESSAGE_TYPE` (Pong消息)，则更新 `last_pong_received_at_clone` 时间戳，
    ///     表明收到了心跳响应。
    ///   - (未来) 其他类型的业务消息将在此处添加相应的处理逻辑。
    ///
    /// # 参数
    /// - `app_handle`: `&AppHandle` - Tauri 应用句柄的引用。
    /// - `ws_msg`: `WsMessage` - 从云端接收到的、已成功解析的 `WsMessage` 实例。
    /// - `_cloud_assigned_client_id_state`: `&Arc<RwLock<Option<String>>>` - (当前未使用) 对云端客户端ID状态的共享引用。
    /// - `last_pong_received_at_clone`: `&Arc<RwLock<Option<DateTime<Utc>>>>` - 对最后Pong时间的共享引用，用于处理 Pong 消息。
    async fn handle_on_message(
        app_handle: &AppHandle,
        ws_msg: WsMessage,
        _cloud_assigned_client_id_state: &Arc<RwLock<Option<String>>>, // 参数保留，但目前在此函数中未使用
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
    ) {
        info!("（静态回调）从云端收到消息: 类型='{}', 消息ID='{}', 时间戳={}, 负载摘要='{}...'", 
               ws_msg.message_type, ws_msg.message_id, ws_msg.timestamp, 
               ws_msg.payload.chars().take(50).collect::<String>() // 日志中只显示负载的前50个字符作为摘要
        );

        // 根据消息类型进行分发处理
        if ws_msg.message_type == common_models::ws_payloads::ECHO_MESSAGE_TYPE { // 检查是否为 Echo 类型的消息
             match serde_json::from_str::<common_models::ws_payloads::EchoPayload>(&ws_msg.payload) { //尝试将负载解析为 EchoPayload
                Ok(echo_payload) => {
                    info!("（静态回调）成功解析 EchoPayload (回声消息负载): {:?}", echo_payload);
                    // 构建 Echo 响应事件的负载，准备发送给前端
                    let response_event_payload = EchoResponseEventPayload { content: echo_payload.content };
                    // 尝试向前端（主窗口）发送 Echo 响应事件
                    if let Err(e) = app_handle.emit_to("main", ECHO_RESPONSE_EVENT, &response_event_payload) {
                         error!("（静态回调）发送 '{}' (Echo响应事件) 到前端失败: {}", ECHO_RESPONSE_EVENT, e);
                    }
                }
                Err(e) => {
                    // 如果解析 EchoPayload 失败，则记录错误
                    error!("（静态回调）解析 EchoPayload (回声消息负载) 失败: {}. 原始负载: {}", e, ws_msg.payload);
                }
            }
        } else if ws_msg.message_type == PONG_MESSAGE_TYPE { // 检查是否为 Pong 类型的消息
            info!("（静态回调）收到来自云端的 Pong (心跳响应) 消息。");
            // 更新最后收到 Pong 的时间戳，用于心跳超时检测
            *last_pong_received_at_clone.write().await = Some(Utc::now());
        }
        // 提示：未来可在此处添加对其他业务消息类型的处理逻辑 (例如，任务更新、状态变更等)
        // else if ws_msg.message_type == "some_other_business_message_type" { ... }
    }

    /// WebSocket 连接关闭时的静态回调处理函数。
    ///
    /// 当 WebSocket 连接因任何原因 (无论是主动断开还是意外关闭) 而终止时，此函数被调用。
    /// 主要职责包括：
    /// - 记录连接关闭的原因。
    /// - 更新内部连接状态 (`is_connected_status`, `cloud_assigned_client_id_state`, `ws_send_channel_clone_for_cleanup`) 为未连接状态。
    /// - 停止心跳任务 (`stop_heartbeat_task_static`)。
    /// - 向前端发送 `WS_CONNECTION_STATUS_EVENT` 事件，通知连接已断开及原因。
    ///
    /// # 参数
    /// - `app_handle`: `&AppHandle` - Tauri 应用句柄的引用。
    /// - `reason`: `Option<String>` - 连接关闭的原因描述，如果可用。
    /// - `is_connected_status`: `&Arc<RwLock<bool>>` - 对连接状态标志的共享引用。
    /// - `cloud_assigned_client_id_state`: `&Arc<RwLock<Option<String>>>` - 对云端客户端ID状态的共享引用。
    /// - `ws_send_channel_clone_for_cleanup`: `&Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>` - 对发送通道的共享引用，用于清理。
    /// - `heartbeat_task_handle_clone`: `&Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>` - 对心跳任务句柄的共享引用。
    /// - `last_pong_received_at_clone`: `&Arc<RwLock<Option<DateTime<Utc>>>>` - 对最后Pong时间的共享引用。
    async fn handle_on_close(
        app_handle: &AppHandle,
        reason: Option<String>,
        is_connected_status: &Arc<RwLock<bool>>,
        cloud_assigned_client_id_state: &Arc<RwLock<Option<String>>>,
        ws_send_channel_clone_for_cleanup: &Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        heartbeat_task_handle_clone: &Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
    ) {
        let reason_msg = reason.unwrap_or_else(|| "连接已关闭，但未提供特定原因。".to_string()); // 提供默认的中文关闭原因
        info!("（静态回调）WebSocket 连接已关闭。原因: {}", reason_msg);
        
        // 更新共享状态：标记为未连接，清除客户端ID，并清理发送通道
        *is_connected_status.write().await = false;
        *cloud_assigned_client_id_state.write().await = None;
        *ws_send_channel_clone_for_cleanup.lock().await = None; // 将发送通道设置为 None

        // 停止心跳任务
        Self::stop_heartbeat_task_static(heartbeat_task_handle_clone, last_pong_received_at_clone).await;

        // 构建连接断开事件的负载
        let event_payload = WsConnectionStatusEvent {
            connected: false,
            error_message: Some(reason_msg), // 传递关闭原因
            client_id: None, // 清除客户端ID
        };
        // 尝试向前端（主窗口）发送连接状态更新事件
        if let Err(e) = app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, &event_payload) {
            error!("（静态回调）发送 '已断开连接' 事件到前端失败: {}", e);
        }
    }

    /// WebSocket 连接发生错误时的静态回调处理函数。
    ///
    /// 当 WebSocket 通信过程中发生错误 (例如，网络问题、协议错误等) 导致连接可能中断或异常时，此函数被调用。
    /// 主要职责与 `handle_on_close` 类似，但通常错误信息更具体：
    /// - 记录详细的错误信息。
    /// - 更新内部连接状态为未连接。
    /// - 停止心跳任务。
    /// - 向前端发送 `WS_CONNECTION_STATUS_EVENT` 事件，通知连接错误及错误详情。
    ///
    /// # 参数 (与 `handle_on_close` 类似，但 `reason` 替换为 `error_message_str`)
    /// - `app_handle`: `&AppHandle`
    /// - `error_message_str`: `String` - 描述错误的具体信息字符串。
    /// - `is_connected_status`: `&Arc<RwLock<bool>>`
    /// - `cloud_assigned_client_id_state`: `&Arc<RwLock<Option<String>>>`
    /// - `ws_send_channel_clone_for_cleanup`: `&Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>`
    /// - `heartbeat_task_handle_clone`: `&Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>`
    /// - `last_pong_received_at_clone`: `&Arc<RwLock<Option<DateTime<Utc>>>>`
    async fn handle_on_error(
        app_handle: &AppHandle,
        error_message_str: String,
        is_connected_status: &Arc<RwLock<bool>>,
        cloud_assigned_client_id_state: &Arc<RwLock<Option<String>>>,
        ws_send_channel_clone_for_cleanup: &Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        heartbeat_task_handle_clone: &Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
    ) {
        error!("（静态回调）WebSocket 连接发生严重错误: {}", error_message_str);
        
        // 更新共享状态：标记为未连接，清除相关状态
        *is_connected_status.write().await = false;
        *cloud_assigned_client_id_state.write().await = None;
        *ws_send_channel_clone_for_cleanup.lock().await = None; // 清理发送通道

        // 停止心跳任务
        Self::stop_heartbeat_task_static(heartbeat_task_handle_clone, last_pong_received_at_clone).await;

        // 构建连接错误事件的负载
        let event_payload = WsConnectionStatusEvent {
            connected: false,
            error_message: Some(error_message_str), // 传递具体的错误信息
            client_id: None,
        };
        // 尝试向前端（主窗口）发送连接状态更新事件
        if let Err(e) = app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, &event_payload) {
            error!("（静态回调）发送 '连接错误' 事件到前端失败: {}", e);
        }
    }

    /// 启动心跳任务的静态辅助函数。
    //!
    /// 此函数负责创建一个新的 Tokio 异步任务，该任务将周期性地：
    /// 1.  等待 `HEARTBEAT_INTERVAL_SECONDS` (心跳间隔)。
    /// 2.  检查当前是否仍处于连接状态。如果已断开，则心跳任务终止。
    /// 3.  检查自上次收到 Pong 消息以来是否已超过 `HEARTBEAT_INTERVAL_SECONDS + PONG_TIMEOUT_SECONDS` (总超时窗口)。
    ///     如果超时，则认为连接已丢失，调用 `handle_on_error` 进行错误处理并终止心跳任务。
    /// 4.  如果一切正常，则构建一个 Ping 消息 (`WsMessage` 类型为 `PING_MESSAGE_TYPE`)。
    /// 5.  尝试通过共享的 `ws_send_channel_clone` (WebSocket发送通道) 将 Ping 消息发送到云端。
    ///     如果发送失败 (例如，通道已关闭)，则调用 `handle_on_error` 并终止心跳任务。
    ///
    /// 在启动新任务之前，它会检查 `heartbeat_task_handle_clone` 中是否已存在一个旧的心跳任务句柄。
    /// 如果存在，则会先中止 (abort) 旧任务，以确保任何时候只有一个心跳任务在运行。
    /// 新创建的任务句柄会被存储回 `heartbeat_task_handle_clone` 中。
    ///
    /// # 参数 (均为 `Arc` 包裹的共享状态，以便在生成的 Tokio 任务中安全使用)
    /// - `app_handle`: `AppHandle` - Tauri 应用句柄。
    /// - `ws_send_channel_clone`: `Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>` - WebSocket 发送通道。
    /// - `heartbeat_task_handle_clone`: `Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>` - 心跳任务句柄的存储位置。
    /// - `last_pong_received_at_clone`: `Arc<RwLock<Option<DateTime<Utc>>>>` - 最后收到 Pong 的时间戳。
    /// - `is_connected_status_clone`: `Arc<RwLock<bool>>` - 当前连接状态标志。
    /// - `cloud_assigned_client_id_clone`: `Arc<RwLock<Option<String>>>` - 云端分配的客户端ID (用于错误处理时传递)。
    async fn start_heartbeat_task_static(
        app_handle: AppHandle,
        ws_send_channel_clone: Arc<TokioMutex<Option<SplitSink<ClientWsStream, TungsteniteMessage>>>>,
        heartbeat_task_handle_clone: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: Arc<RwLock<Option<DateTime<Utc>>>>,
        is_connected_status_clone: Arc<RwLock<bool>>, 
        cloud_assigned_client_id_clone: Arc<RwLock<Option<String>>>,
    ) {
        info!("（静态心跳任务辅助函数）请求启动心跳任务 (start_heartbeat_task_static)。");
        // ---- 使用 Mutex 保护对 heartbeat_task_handle_clone 的访问 ----
        {
            let mut task_handle_guard = heartbeat_task_handle_clone.lock().await; // 获取互斥锁
            if let Some(existing_handle) = task_handle_guard.take() { // .take() 会移除 Option 中的值并返回它
                info!("（静态心跳任务辅助函数）检测到已存在的心跳任务，正在尝试中止旧任务...");
                existing_handle.abort(); // 中止已存在的任务
                // 注意：中止任务后，我们不立即等待它结束 (join)，因为这可能阻塞当前流程。
                // Tokio 任务在被中止后，其内部的 .await 点会返回错误，从而使其自然退出。
            }
            // task_handle_guard 在此作用域结束时自动释放锁
        }
        // ---- Mutex 保护结束 ----

        info!("（静态心跳任务辅助函数）准备启动新的心跳任务，心跳间隔: {} 秒, Pong超时检测窗口: {} 秒 (基于心跳间隔计算)。", HEARTBEAT_INTERVAL_SECONDS, HEARTBEAT_INTERVAL_SECONDS + PONG_TIMEOUT_SECONDS);
        
        // 克隆所有需要在新生成的 Tokio 任务中使用的 Arc 引用。
        // 这是因为 `tokio::spawn` 要求其闭包是 `'static` 生命周期，并且捕获的变量必须拥有所有权。
        // Arc 的克隆是轻量级的 (只增加引用计数)。
        let app_handle_for_spawn = app_handle.clone();
        let ws_send_channel_for_spawn = ws_send_channel_clone.clone();
        let heartbeat_task_handle_for_spawn = heartbeat_task_handle_clone.clone(); // 这个 Arc 实例将被移动到新任务中，用于自我管理 (如果需要)
        let last_pong_for_spawn = last_pong_received_at_clone.clone();
        let is_connected_for_spawn = is_connected_status_clone.clone();
        let cloud_id_for_spawn = cloud_assigned_client_id_clone.clone();

        // 创建并启动新的 Tokio 异步任务作为心跳循环
        let new_task = tokio::spawn(async move {
            info!("（心跳任务循环）新心跳任务已启动。进入主循环。");
            loop {
                info!("（心跳任务循环）循环顶部，即将休眠 {} 秒作为心跳间隔。", HEARTBEAT_INTERVAL_SECONDS);
                tokio::time::sleep(Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS)).await;
                info!("（心跳任务循环）休眠结束，准备执行心跳检查和发送 Ping。");

                // 在执行任何操作前，先检查当前是否仍处于连接状态
                let connected_status = *is_connected_for_spawn.read().await;
                info!("（心跳任务循环）检查连接状态: {}", if connected_status { "已连接" } else { "未连接" });
                if !connected_status {
                    info!("（心跳任务循环）检测到连接已断开 (is_connected_status 为 false)，心跳任务将自行终止。");
                    break; // 如果已断开连接，则退出心跳循环
                }

                // 检查 Pong 超时
                let now = Utc::now();
                let last_pong_opt = *last_pong_for_spawn.read().await;
                if let Some(last_pong_time) = last_pong_opt {
                    // 计算从上次收到 Pong 到现在的时间差 (秒)
                    let seconds_since_last_pong = now.signed_duration_since(last_pong_time).num_seconds();
                    // 总的超时窗口是：一个心跳间隔 (等待下次Ping) + 一个Pong超时时间 (等待本次Ping的Pong)
                    let pong_timeout_window_seconds = (HEARTBEAT_INTERVAL_SECONDS + PONG_TIMEOUT_SECONDS) as i64;
                    
                    info!("（心跳任务循环）Pong 超时检查: 上次Pong时间 {:?}, 当前时间 {:?}, 已过秒数 {}, 超时窗口 {}s", 
                           last_pong_time, now, seconds_since_last_pong, pong_timeout_window_seconds);

                    if seconds_since_last_pong > pong_timeout_window_seconds {
                        error!(
                            "（心跳任务循环）检测到 Pong 超时！上次收到 Pong 是在: {:?} ({}秒前), 当前时间: {:?}. 总超时窗口 {}秒已过。将主动断开连接并报告错误。", 
                            last_pong_time, seconds_since_last_pong, now, pong_timeout_window_seconds
                        );
                        // 标记为未连接。注意：这里直接修改 is_connected_status，
                        // 其他依赖此状态的地方 (如主连接的接收循环) 也可能因此而终止。
                        *is_connected_for_spawn.write().await = false; 
                        
                        // 调用静态错误处理函数，通知应用连接因 Pong 超时而断开
                        // 这里传递的是已为 spawn 克隆的 Arc 引用
                        Self::handle_on_error(
                            &app_handle_for_spawn, 
                            "Pong 响应超时，认为与云端的连接已丢失。".to_string(),
                            &is_connected_for_spawn,
                            &cloud_id_for_spawn,
                            &ws_send_channel_for_spawn, 
                            &heartbeat_task_handle_for_spawn, 
                            &last_pong_for_spawn, 
                        ).await;
                        break; // Pong 超时，退出心跳循环
                    }
                } else {
                    // 如果 last_pong_opt 是 None，这通常发生在连接刚建立，第一个 Pong 还没收到，或者状态被重置了。
                    // 此时不应判断为超时，心跳任务会继续尝试发送 Ping。
                    info!("（心跳任务循环）尚未收到过 Pong (last_pong_opt 为 None)，本次不进行超时检查，将继续发送 Ping。");
                }

                info!("（心跳任务循环）准备发送 Ping 消息到云端...");
                // 构建 Ping 消息。PingPayload 通常为空结构体，仅用于满足 WsMessage 的 payload 类型要求。
                match WsMessage::new(PING_MESSAGE_TYPE.to_string(), &PingPayload {}) {
                    Ok(ping_message) => {
                        // 获取对发送通道的互斥访问锁
                        let mut sender_guard = ws_send_channel_for_spawn.lock().await; // 使用为spawn克隆的 ws_send_channel Arc
                        if let Some(sender) = sender_guard.as_mut() { // 检查发送通道是否仍然有效 (Option 是否为 Some)
                            // 将 WsMessage 序列化为 JSON 字符串，然后包装为 TungsteniteMessage::Text 以发送
                            match sender.send(TungsteniteMessage::Text(serde_json::to_string(&ping_message).unwrap_or_default())).await {
                                Ok(_) => info!("（心跳任务循环）Ping 消息已成功发送到云端。消息ID: {}", ping_message.message_id), // 原为 debug 日志，提升为 info 以便观察
                                Err(e) => {
                                    error!("（心跳任务循环）通过 WebSocket 发送 Ping 消息失败: {}. 可能连接已在此期间断开。", e);
                                    *is_connected_for_spawn.write().await = false; // 标记为未连接
                                    // 调用 handle_on_error 进行统一的错误处理和状态清理
                                    Self::handle_on_error(
                                        &app_handle_for_spawn, 
                                        format!("发送Ping消息时连接中断: {}", e),
                                        &is_connected_for_spawn,
                                        &cloud_id_for_spawn,
                                        &ws_send_channel_for_spawn, 
                                        &heartbeat_task_handle_for_spawn, 
                                        &last_pong_for_spawn, 
                                    ).await;
                                    break; // 发送失败，退出心跳循环
                                }
                            }
                        } else {
                            info!("（心跳任务循环）WebSocket 发送通道在尝试发送 Ping 时已不可用 (为 None)。可能连接已在本轮循环早期断开。心跳任务将终止。");
                            *is_connected_for_spawn.write().await = false; // 确保标记为未连接
                            break; // 发送通道无效，退出心跳循环
                        }
                    }
                    Err(e) => {
                        // WsMessage::new 理论上不应轻易失败，除非消息类型或负载结构有问题，或者内存分配失败。
                        error!("（心跳任务循环）构建 Ping 消息 (WsMessage) 失败: {}. 这是一个严重问题，请检查代码。", e);
                        // 即使构建消息失败，也应考虑是否需要停止心跳或标记错误，因为这可能表示系统处于不稳定状态。
                        // 此处选择继续循环尝试，但实际项目中可能需要更复杂的错误处理策略。
                    }
                }
            }
            info!("（心跳任务循环）心跳任务已正常结束或因错误/断开而终止。");
        });

        // 在 Tokio 任务生成后，重新获取锁以存储新任务的句柄。
        // 这使得服务可以在需要时 (例如，在断开连接或启动新的心跳任务时) 中止这个心跳任务。
        let mut task_handle_guard = heartbeat_task_handle_clone.lock().await;
        *task_handle_guard = Some(new_task);
        info!("（静态心跳任务辅助函数）新的心跳任务已成功启动并存储其句柄。");
    }

    /// 停止心跳任务的静态辅助函数。
    ///
    /// 此函数负责安全地停止当前正在运行的心跳任务 (如果存在)。
    /// 它通过获取对 `heartbeat_task_handle_clone` (心跳任务句柄存储位置) 的互斥访问权，
    /// 取出可能存在的任务句柄 (`JoinHandle`)，并调用其 `abort()` 方法来请求任务终止。
    /// 同时，它也会重置 `last_pong_received_at_clone` (最后收到Pong的时间戳) 为 `None`，
    /// 以清除与已停止心跳任务相关的状态。
    ///
    /// 此函数设计为幂等的：如果多次调用，或者在没有活动心跳任务时调用，它不会产生错误，
    /// 只是确保心跳任务最终被停止且相关状态被清理。
    ///
    /// # 参数
    /// - `heartbeat_task_handle_clone`: `&Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>` - 
    ///   对存储心跳任务句柄的 `Option` 的共享、互斥访问引用。
    /// - `last_pong_received_at_clone`: `&Arc<RwLock<Option<DateTime<Utc>>>>` - 
    ///   对存储最后接收 Pong 时间戳的 `Option` 的共享、读写锁访问引用。
    async fn stop_heartbeat_task_static(
        heartbeat_task_handle_clone: &Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
        last_pong_received_at_clone: &Arc<RwLock<Option<DateTime<Utc>>>>,
    ) {
        info!("（静态心跳任务辅助函数）请求停止心跳任务 (stop_heartbeat_task_static)... ");
        // 获取对心跳任务句柄的独占访问锁
        let mut guard = heartbeat_task_handle_clone.lock().await;
        // .take() 会移除 Option 中的 JoinHandle (如果存在的话)，并将其所有权转移给这里的 handle 变量，
        // 同时将 Option 本身设置为 None。这确保了即使在异步中止操作中，我们也能正确处理句柄。
        if let Some(handle) = guard.take() { 
            handle.abort(); // 请求 Tokio 任务终止。任务将在下一个 .await 点因 Aborted 错误而退出。
            info!("（静态心跳任务辅助函数）已向活动的心跳任务发送中止信号。");
        } else {
            info!("（静态心跳任务辅助函数）当前没有活动的或已记录的心跳任务需要中止。");
        }
        // 清理最后接收Pong的时间戳状态，因为它与已停止 (或即将停止) 的心跳任务相关。
        *last_pong_received_at_clone.write().await = None; 
        info!("（静态心跳任务辅助函数）最后接收Pong的时间戳已重置为 None。");
    }

    /// 尝试异步连接到指定的 WebSocket 服务器 URL。
    ///
    /// 此方法是 `WebSocketClientService` 的核心入口点之一，用于启动与云端服务的 WebSocket 连接过程。
    /// 它执行以下主要步骤：
    /// 1.  **清理旧状态**: 首先，它会尝试停止任何可能仍在运行的旧心跳任务，以避免冲突。
    /// 2.  **中止现有连接**: 如果当前已存在一个活动的连接任务 (通过 `self.connection_task_handle` 判断)，
    ///     该旧任务会被中止 (`abort()`)。同时，会调用 `Self::handle_on_close` 来确保与旧连接相关的
    ///     所有状态 (如 `is_connected_status`, `cloud_assigned_client_id`, `ws_send_channel`) 被正确清理，
    ///     并向前端发送相应的连接关闭事件。
    /// 3.  **重置连接状态**: 显式地将 `self.is_connected_status` 设置为 `false`，确保在尝试新连接前处于明确的未连接状态。
    /// 4.  **通知前端**: 向前端发送一个 `WS_CONNECTION_STATUS_EVENT` 事件，负载中包含错误消息，
    ///     指示应用正在"尝试连接到 {url_str}"，这有助于向用户提供即时反馈。
    /// 5.  **执行连接**: 调用 `rust_websocket_utils::client::transport::connect_client` 函数，
    ///     传入目标 `url_str` 来实际执行底层的 WebSocket 连接握手。
    /// 6.  **连接成功处理**:
    ///     a.  如果 `connect_client` 返回 `Ok(client_conn)`，表示连接成功建立。
    ///     b.  从 `client_conn` 中分离出 WebSocket 的发送半部分 (`ws_sender`) 和接收半部分 (`ws_receiver`)。
    ///     c.  将 `ws_sender` 存储到 `self.ws_send_channel` 中，以便后续可以通过 `send_ws_message` 方法发送消息。
    ///     d.  立即调用 `Self::handle_on_open` 静态回调，传入必要的共享状态引用，以处理连接打开后的初始化逻辑
    ///         (如更新状态、发送连接成功事件给前端、启动心跳任务等)。
    ///     e.  克隆所有需要在新生成的 Tokio 消息接收任务中使用的 `Arc` 包装的共享状态。
    ///     f.  使用 `tokio::spawn` 创建一个新的异步任务，专门负责从 `ws_receiver` 持续接收消息。
    ///         - 此任务内部使用一个 `loop` 和 `tokio::select!` 来等待消息。
    ///         - 当成功接收到 `WsMessage` 时，调用 `Self::handle_on_message` 进行处理。
    ///         - 如果接收过程中发生错误 (`Some(Err(ws_err))`)，调用 `Self::handle_on_error`。
    ///         - 如果接收流正常结束 (`None`)，表示连接由对方关闭，调用 `Self::handle_on_close`。
    ///         - 在任何导致循环中断的情况下 (错误或关闭)，接收任务会自行终止。
    ///     g.  将新创建的接收任务的 `JoinHandle` 存储到 `self.connection_task_handle` 中，以便将来可以管理它。
    ///     h.  方法返回 `Ok(())` 表示连接过程已成功启动 (注意，消息接收和心跳是后台任务)。
    /// 7.  **连接失败处理**:
    ///     a.  如果 `connect_client` 返回 `Err(e)`，表示连接尝试失败。
    ///     b.  构造详细的错误消息，并调用 `Self::handle_on_error` 静态回调来处理错误状态的更新和前端事件通知。
    ///     c.  方法返回 `Err(String)`，其中包含格式化的错误消息。
    ///
    /// # 参数
    /// - `url_str`: `&str` - 要连接的 WebSocket 服务器的完整 URL 字符串。
    ///
    /// # 返回值
    /// - `Result<(), String>`:
    ///   - `Ok(())`: 如果连接过程已成功启动 (即，连接建立，消息接收和心跳任务已安排)。
    ///   - `Err(String)`: 如果在任何阶段 (例如，解析URL、底层连接失败、内部状态更新失败) 发生错误，
    ///     则返回包含描述性错误信息的字符串。
    pub async fn connect(&self, url_str: &str) -> Result<(), String> {
        info!("WebSocketClientService (WebSocket客户端服务): 收到连接请求，目标 URL: {}", url_str);

        // 步骤 0: 尝试停止任何可能正在运行的旧心跳任务。
        // 这是一个防御性措施，因为正常情况下，旧心跳任务应该在旧连接关闭时由 handle_on_close/error 停止。
        // 但为确保在任何情况下开始新连接时，旧心跳不会干扰，这里显式调用一次。
        info!("WebSocketClientService.connect: 执行防御性操作 - 停止任何可能残留的旧心跳任务。");
        Self::stop_heartbeat_task_static(
            &self.heartbeat_task_handle, // 使用服务自身的 heartbeat_task_handle Arc
            &self.last_pong_received_at  // 使用服务自身的 last_pong_received_at Arc
        ).await;

        // 步骤 1: 如果当前已存在活动的连接任务，则先中止它，并清理相关状态。
        // 获取对 connection_task_handle 的独占访问锁
        let mut previous_task_handle_guard = self.connection_task_handle.lock().await;
        if let Some(previous_task_handle) = previous_task_handle_guard.take() { // .take() 移除并返回 Option 中的值
            info!("WebSocketClientService.connect: 检测到已存在的旧连接任务，正在中止该旧任务...");
            previous_task_handle.abort(); // 请求中止旧的连接任务
            // 即使任务被中止，其内部的清理逻辑 (如 on_close/on_error) 可能需要时间执行或可能不完整。
            // 因此，在这里显式调用 handle_on_close 来确保状态被立即、彻底地清理，
            // 并通知前端旧连接已因"发起新连接"而被中止。
            info!("WebSocketClientService.connect: 旧连接任务已中止，现在调用 handle_on_close 清理旧连接状态并通知前端。");
            Self::handle_on_close(
                &self.app_handle, // 应用句柄
                Some("由于发起新的连接请求，旧的 WebSocket 连接已被中止。".to_string()), // 关闭原因
                &self.is_connected_status, // 连接状态 Arc
                &self.cloud_assigned_client_id, // 客户端ID Arc
                &self.ws_send_channel, // 发送通道 Arc
                &self.heartbeat_task_handle, // 心跳任务句柄 Arc (handle_on_close 内部会调用 stop_heartbeat_task_static)
                &self.last_pong_received_at,  // 最后Pong时间 Arc (handle_on_close 内部会清理)
            ).await;
        } else {
            info!("WebSocketClientService.connect: 未检测到活动的旧连接任务，无需中止。");
        }
        // 释放锁 previous_task_handle_guard
        
        // 步骤 2: 确保在尝试新连接之前，内部状态明确为"未连接"。
        // 即使 handle_on_close 已经被调用，这里再次设置为 false 是一种防御性编程，确保状态一致性。
        *self.is_connected_status.write().await = false; 
        info!("WebSocketClientService.connect: 内部连接状态已明确设置为 '未连接'。");


        // 步骤 3: 向前端发送一个事件，表明正在尝试连接。
        // 这为用户提供了即时反馈，告知应用正在进行连接操作。
        let attempt_payload = WsConnectionStatusEvent {
            connected: false, // 当前仍是未连接状态
            error_message: Some(format!("正在尝试连接到 WebSocket 服务器: {}", url_str)), // 提示信息
            client_id: None, // 此时还没有客户端ID
        };
        if let Err(e) = self.app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, &attempt_payload) {
            error!("WebSocketClientService.connect: 发送 '正在尝试连接' 事件到前端失败: {}", e);
            // 注意：即使发送事件失败，通常也应继续尝试连接，因为这不影响核心连接逻辑。
        } else {
            info!("WebSocketClientService.connect: 已向前端发送 '正在尝试连接' 事件。");
        }

        // 步骤 4: 调用 rust_websocket_utils 中的 connect_client 函数执行实际的 WebSocket 连接尝试。
        // 这是一个异步操作，会进行网络握手等。
        info!("WebSocketClientService.connect: 即将调用 connect_client 尝试连接到 URL: {}", url_str);
        match connect_client(url_str.to_string()).await { // 等待连接结果
            Ok(client_conn) => { // 连接成功建立
                info!("WebSocketClientService.connect: 使用 connect_client 成功连接到云端服务器: {}", url_str);
                // 从返回的 ClientConnection 中解构出发送流 (ws_sender) 和接收流 (ws_receiver)
                let ClientConnection { ws_sender, mut ws_receiver } = client_conn;
                
                // 将获取到的 WebSocket 发送流 (ws_sender) 存储到服务自身的共享状态中。
                // 加锁以确保线程安全。
                *self.ws_send_channel.lock().await = Some(ws_sender);
                info!("WebSocketClientService.connect: WebSocket 发送通道已成功存储。");
                
                // 连接成功建立后，立即调用静态的 on_open 回调函数处理后续逻辑。
                // 注意：此时 cloud_assigned_client_id 通常为 None，因为它一般是在连接建立后，
                // 通过一个特定的注册/认证消息从云端服务获取的。
                info!("WebSocketClientService.connect: 即将调用静态 handle_on_open 回调函数...");
                Self::handle_on_open(
                    &self.app_handle, // 应用句柄
                    None, // 初始 client_id 为 None
                    &self.is_connected_status, // 连接状态 Arc
                    &self.cloud_assigned_client_id, // 客户端ID Arc
                    self.ws_send_channel.clone(), // 发送通道 Arc (克隆给心跳任务使用)
                    self.heartbeat_task_handle.clone(), // 心跳任务句柄 Arc (克隆给心跳任务使用)
                    self.last_pong_received_at.clone(), // 最后Pong时间 Arc (克隆给心跳任务使用)
                ).await;
                info!("WebSocketClientService.connect: 静态 handle_on_open 回调函数执行完毕。");

                // 为新生成的 Tokio 消息接收任务克隆所有必需的 Arc<...> 共享状态引用。
                // 这是因为新任务将独立运行，需要拥有这些状态的所有权 (通过 Arc 共享)。
                let app_handle_clone_for_receiver = self.app_handle.clone();
                let is_connected_status_clone_for_receiver = self.is_connected_status.clone();
                let cloud_assigned_client_id_clone_for_receiver = self.cloud_assigned_client_id.clone();
                let ws_send_channel_clone_for_receiver_callbacks = self.ws_send_channel.clone();
                // 克隆心跳相关的状态，这些状态可能会在接收任务的错误处理或关闭处理回调中被访问或修改。
                let heartbeat_task_handle_clone_for_receiver_callbacks = self.heartbeat_task_handle.clone();
                let last_pong_received_at_clone_for_receiver_callbacks = self.last_pong_received_at.clone();
                info!("WebSocketClientService.connect: 已为消息接收任务克隆所有必要的共享状态 Arc 引用。");


                // 步骤 5: 启动一个新的 Tokio 异步任务，专门负责持续从 WebSocket 连接的接收端读取消息。
                info!("WebSocketClientService.connect: 准备启动 Tokio 异步任务以处理 WebSocket 消息接收...");
                let connection_task = tokio::spawn(async move {
                    info!("（WebSocket消息接收任务）任务已启动，进入消息接收循环。");
                    loop { // 无限循环，持续等待和处理传入的消息
                        // 使用 tokio::select! 宏来同时监听消息接收和可能的未来中断信号 (如果需要)。
                        // `biased` 关键字确保 select! 优先检查第一个分支 (即 receive_message)。
                        tokio::select! {
                            biased; // 优先处理消息接收
                            
                            // 尝试从 WebSocket 接收流 (ws_receiver) 中异步接收下一条消息。
                            // `receive_message` 是 `rust_websocket_utils` 提供的辅助函数，
                            // 它处理底层的 Tungstenite 消息并将其转换为 `WsMessage`。
                            msg_option = receive_message(&mut ws_receiver) => { // 等待消息
                                match msg_option { // 对接收结果进行模式匹配
                                    Some(Ok(ws_msg)) => { // 成功接收并解析为一个 WsMessage
                                        info!("（WebSocket消息接收任务）成功从云端接收到一条消息，类型: {}。将调用 handle_on_message 处理。", ws_msg.message_type);
                                        // 调用静态回调函数 handle_on_message 来处理这条消息。
                                        // 注意传递的是为接收任务克隆的各种状态 Arc 引用。
                                        Self::handle_on_message(
                                            &app_handle_clone_for_receiver, 
                                            ws_msg, 
                                            &cloud_assigned_client_id_clone_for_receiver, 
                                            &last_pong_received_at_clone_for_receiver_callbacks
                                        ).await;
                                    }
                                    Some(Err(ws_err)) => { // 在尝试接收或解析消息时发生错误
                                        let err_msg = format!("（WebSocket消息接收任务）从 WebSocket 流接收或解析消息时发生错误: {:?}", ws_err);
                                        error!("{}", err_msg);
                                        // 调用静态回调函数 handle_on_error 来处理这个错误。
                                        Self::handle_on_error(
                                            &app_handle_clone_for_receiver, 
                                            err_msg, 
                                            &is_connected_status_clone_for_receiver, 
                                            &cloud_assigned_client_id_clone_for_receiver, 
                                            &ws_send_channel_clone_for_receiver_callbacks, 
                                            &heartbeat_task_handle_clone_for_receiver_callbacks, 
                                            &last_pong_received_at_clone_for_receiver_callbacks
                                        ).await;
                                        break; // 发生不可恢复的错误，终止消息接收循环
                                    }
                                    None => { // WebSocket 接收流已正常关闭 (例如，对方发送了 Close 帧)
                                        info!("（WebSocket消息接收任务）WebSocket 接收流已由对方正常关闭。将调用 handle_on_close 处理。");
                                        // 调用静态回调函数 handle_on_close 来处理连接关闭。
                                        Self::handle_on_close(
                                            &app_handle_clone_for_receiver, 
                                            Some("连接已由云端服务器主动关闭。".to_string()), 
                                            &is_connected_status_clone_for_receiver, 
                                            &cloud_assigned_client_id_clone_for_receiver, 
                                            &ws_send_channel_clone_for_receiver_callbacks, 
                                            &heartbeat_task_handle_clone_for_receiver_callbacks, 
                                            &last_pong_received_at_clone_for_receiver_callbacks
                                        ).await;
                                        break; // 连接已关闭，终止消息接收循环
                                    }
                                }
                            }
                            // 这个休眠分支主要用于确保 select! 宏在某些情况下 (例如，如果所有其他分支的 Future 都是 Pending)
                            // 不会永久阻塞。对于一个只等待消息的循环，它理论上不应被频繁触发。
                            // 如果需要实现任务的主动中止机制 (例如，通过一个 channel 发送中止信号)，
                            // 可以在这里添加一个监听该 channel 的分支。
                            _ = tokio::time::sleep(Duration::from_secs(u64::MAX)) => { 
                                // 理论上不应执行到此，除非 receive_message 永久阻塞且没有其他事件。
                                // 包含此分支是为了 select! 宏的完整性，并提示可以扩展为更复杂的任务管理。
                                warn!("（WebSocket消息接收任务）select! 宏中的超长休眠分支被意外触发，这可能表示一个逻辑问题。");
                            }
                        }
                    }
                    info!("（WebSocket消息接收任务）消息接收循环已结束。任务即将终止。");
                    // 注意：当此任务因任何原因 (正常关闭、错误、或被中止) 结束时，
                    // `connection_task_handle` 中持有的 `JoinHandle` 仍然存在，直到被显式 `.take()`。
                    // 在 `connect` 方法开始处或 `disconnect` 方法中，会处理旧的 `JoinHandle`。
                    // 此处无需额外操作 `connection_task_handle`。
                });
                info!("WebSocketClientService.connect: WebSocket 消息接收任务已成功启动。");

                // 将新创建的连接任务 (消息接收任务) 的 JoinHandle 存储到服务自身的共享状态中。
                // 这允许服务在后续操作 (如断开连接或发起新连接) 中管理此任务的生命周期。
                *self.connection_task_handle.lock().await = Some(connection_task);
                info!("WebSocketClientService.connect: 新连接任务的 JoinHandle 已成功存储。");
                
                Ok(()) // 连接过程成功启动
            }
            Err(e) => { // 连接到指定 URL 失败
                let err_msg = format!("WebSocketClientService.connect: 尝试连接到 URL '{}' 失败: {:?}", url_str, e);
                error!("{}", err_msg);
                // 调用静态回调函数 handle_on_error 来处理连接失败的情况，
                // 这会更新内部状态并向前端发送错误事件。
                Self::handle_on_error(
                    &self.app_handle, // 应用句柄
                    err_msg.clone(), // 传递克隆的错误消息
                    &self.is_connected_status, // 连接状态 Arc
                    &self.cloud_assigned_client_id, // 客户端ID Arc
                    &self.ws_send_channel, // 发送通道 Arc
                    &self.heartbeat_task_handle, // 心跳任务句柄 Arc
                    &self.last_pong_received_at,  // 最后Pong时间 Arc
                ).await;
                Err(err_msg) // 将错误信息返回给调用者 (通常是 Tauri 命令)
            }
        }
    }

    /// 主动断开当前的 WebSocket 连接。
    ///
    /// 此方法负责优雅地关闭当前活动的 WebSocket 连接，并清理所有相关资源。
    /// 它执行以下主要步骤：
    /// 1.  **记录断开请求**: 记录一条信息日志，表明服务收到了断开连接的请求。
    /// 2.  **停止心跳任务**: 调用 `Self::stop_heartbeat_task_static` 来立即中止并清理正在运行的心跳任务。
    ///     这是必要的，以防止心跳任务在连接关闭后继续尝试发送 Ping 或检查 Pong 超时。
    /// 3.  **中止连接任务**: 获取对 `self.connection_task_handle` (存储消息接收任务句柄的 `Arc<TokioMutex<...>>`) 的互斥锁。
    ///     如果存在一个活动的连接任务 (`Some(handle)`），则调用其 `abort()` 方法来请求任务终止。
    ///     `.take()` 方法会移除并返回句柄，将 `self.connection_task_handle` 内部的 `Option` 设置为 `None`。
    /// 4.  **等待任务结束 (可选但推荐)**: 在调用 `abort()` 之后，可以（也通常推荐）对任务句柄调用 `.await` 来等待任务实际完成其清理并退出。
    ///     虽然 `abort()` 会使任务在下一个 `.await` 点因 `Aborted` 错误而退出，但等待可以确保在 `disconnect` 方法返回前，
    ///     所有与该任务相关的资源（如套接字）都已释放。如果任务在被中止时正在执行一些关键的清理操作，等待可以避免竞态条件。
    ///     然而，如果任务的清理过程可能非常耗时或阻塞，或者在某些情况下不希望 `disconnect` 调用被阻塞，则可以省略 `.await`。
    ///     当前实现中，注释掉了 `.await`，表示采用非阻塞式中止。但需要注意，这可能导致资源未完全释放时方法就返回。
    /// 5.  **清理发送通道**: 获取对 `self.ws_send_channel` (存储 WebSocket 发送半部分的 `Arc<TokioMutex<...>>`) 的互斥锁，
    ///     并将其内部的 `Option` 设置为 `None`。这会丢弃发送流的句柄，从而关闭 WebSocket 的发送端，并释放相关资源。
    /// 6.  **更新连接状态**: 将 `self.is_connected_status` (共享的连接状态标志 `Arc<RwLock<bool>>`) 设置为 `false`。
    /// 7.  **清除客户端 ID**: 将 `self.cloud_assigned_client_id` (共享的客户端ID `Arc<RwLock<Option<String>>>`) 设置为 `None`。
    /// 8.  **通知前端**: 构建一个 `WsConnectionStatusEvent` 负载，其中 `connected` 为 `false`，`error_message` 包含主动断开连接的原因。
    ///     通过 `self.app_handle.emit_to("main", ...)` 向前端发送此事件，通知用户连接已断开。
    /// 9.  **返回结果**: 返回 `Ok(())` 表示断开操作已成功执行 (或至少已成功发起)。
    ///
    /// 此方法设计为即使在没有活动连接时调用也是安全的 (幂等性)。
    ///
    /// # 返回值
    /// - `Result<(), String>`: 
    ///   - `Ok(())` 总是返回，表示断开操作已处理。
    ///   - （当前实现永不返回 `Err`，但未来可以扩展以处理例如清理资源时的潜在错误）
    pub async fn disconnect(&self) -> Result<(), String> {
        info!("WebSocketClientService: 收到断开连接请求。");

        // 步骤 1: 停止心跳任务
        info!("WebSocketClientService.disconnect: 正在停止心跳任务...");
        Self::stop_heartbeat_task_static(
            &self.heartbeat_task_handle, 
            &self.last_pong_received_at
        ).await;
        info!("WebSocketClientService.disconnect: 心跳任务已停止。");

        // 步骤 2: 中止主连接任务 (消息接收循环)
        info!("WebSocketClientService.disconnect: 正在尝试中止主连接任务 (消息接收循环)... ");
        let mut task_handle_guard = self.connection_task_handle.lock().await;
        if let Some(task_handle) = task_handle_guard.take() { // .take() 移除并返回 Option 中的值
            task_handle.abort(); // 请求中止任务
            info!("WebSocketClientService.disconnect: 已向主连接任务发送中止信号。");
            // 可选：等待任务实际结束。如果任务的清理很重要，应该等待。
            // match task_handle.await {
            //     Ok(_) => info!("WebSocketClientService.disconnect: 主连接任务已成功中止并结束。"),
            //     Err(e) if e.is_cancelled() => info!("WebSocketClientService.disconnect: 主连接任务被中止取消，符合预期。"),
            //     Err(e) => error!("WebSocketClientService.disconnect: 等待主连接任务结束时发生错误: {:?}", e),
            // }
        } else {
            info!("WebSocketClientService.disconnect: 未找到活动的连接任务需要中止。");
        }
        // 锁 task_handle_guard 在此作用域结束时自动释放

        // 步骤 3: 清理 WebSocket 发送通道
        info!("WebSocketClientService.disconnect: 正在清理 WebSocket 发送通道...");
        *self.ws_send_channel.lock().await = None;
        info!("WebSocketClientService.disconnect: WebSocket 发送通道已清理。");

        // 步骤 4: 更新共享的连接状态 和 清除客户端ID
        info!("WebSocketClientService.disconnect: 正在更新内部连接状态并清除客户端ID...");
        *self.is_connected_status.write().await = false;
        *self.cloud_assigned_client_id.write().await = None;
        info!("WebSocketClientService.disconnect: 内部连接状态已更新为 '未连接'，客户端ID已清除。");

        // 步骤 5: 向前端发送连接已断开的事件
        let event_payload = WsConnectionStatusEvent {
            connected: false,
            error_message: Some("WebSocket 连接已由用户或程序主动断开。".to_string()),
            client_id: None, // 因为已断开连接
        };
        if let Err(e) = self.app_handle.emit_to("main", WS_CONNECTION_STATUS_EVENT, &event_payload) {
            error!("WebSocketClientService.disconnect: 发送 '已断开连接' 事件到前端失败: {}", e);
        }
        info!("WebSocketClientService.disconnect: '已断开连接' 事件已发送 (或尝试发送) 到前端。");

        Ok(())
    }

    /// 异步检查当前 WebSocket 是否已连接。
    ///
    /// 此方法通过读取共享的 `is_connected_status` 标志 (`Arc<RwLock<bool>>`) 
    /// 来确定当前的连接状态。由于状态是并发安全的，此方法可以在任何时候被调用
    /// 以获取最新的连接信息。
    ///
    /// # 返回值
    /// - `bool`: 如果已连接则返回 `true`，否则返回 `false`。
    pub async fn is_connected(&self) -> bool {
        *self.is_connected_status.read().await // 读取共享状态的值
    }

    /// 异步发送一条 `WsMessage` 到已连接的 WebSocket 服务器。
    ///
    /// 此方法负责将一个应用层定义的 `WsMessage` 对象序列化为 JSON 字符串，
    /// 然后通过当前活动的 WebSocket 连接的发送通道 (`ws_send_channel`) 将其发送出去。
    ///
    /// # 前提条件
    /// - WebSocket 连接必须已经成功建立，并且 `self.ws_send_channel` 中必须包含一个有效的
    ///   `SplitSink<ClientWsStream, TungsteniteMessage>` 实例。
    /// - 如果当前未连接或发送通道无效，则方法会返回错误。
    ///
    /// # 主要步骤
    /// 1.  **检查连接状态**: 首先，它会检查 `self.is_connected_status`。如果为 `false`，则立即返回一个
    ///     内容为"客户端未连接到 WebSocket 服务器。"的错误 (`Err(String)`)。
    /// 2.  **序列化消息**: 将传入的 `message: WsMessage` 对象使用 `serde_json::to_string` 序列化为 JSON 字符串。
    ///     如果序列化失败 (虽然对于正确定义的 `WsMessage` 和其载荷结构这很少见)，则返回一个
    ///     包含序列化错误信息的错误 (`Err(String)`)。
    /// 3.  **获取发送通道**: 获取对 `self.ws_send_channel` (类型为 `Arc<TokioMutex<Option<SplitSink<...>>>>`) 的互斥锁。
    /// 4.  **发送消息**: 
    ///     a.  如果 `ws_send_channel` 内部的 `Option` 是 `Some(sender)`，表示发送通道有效。
    ///     b.  将序列化后的 JSON 字符串包装成 `TungsteniteMessage::Text`。
    ///     c.  调用 `sender.send(...).await` 来实际异步发送消息。
    ///     d.  如果发送成功 (`Ok(_)`），则记录一条调试日志 (或信息日志)，并返回 `Ok(())`。
    ///     e.  如果发送失败 (`Err(e)`)，这通常表示底层 WebSocket 连接已断开或出现问题。
    ///         此时，它会记录一个错误日志，然后关键地，会主动调用 `self.disconnect().await` 
    ///         来确保执行完整的连接断开和状态清理流程 (包括停止心跳、通知前端等)。
    ///         然后，它返回一个包含发送错误信息的错误 (`Err(String)`)。
    /// 5.  **处理无效发送通道**: 如果在获取锁后发现 `ws_send_channel` 内部的 `Option` 是 `None` 
    ///     (即使 `is_connected_status` 可能声称已连接，这表示状态不一致或正在断开过程中)，
    ///     则返回一个内容为"WebSocket 发送通道不可用，可能连接正在关闭或已关闭。"的错误 (`Err(String)`)。
    ///
    /// # 参数
    /// - `message`: `WsMessage` - 要发送的 WebSocket 消息对象。
    ///
    /// # 返回值
    /// - `Result<(), String>`:
    ///   - `Ok(())`: 如果消息已成功排队等待发送 (注意：这不保证消息已实际到达服务器)。
    ///   - `Err(String)`: 如果发生任何错误 (未连接、序列化失败、发送失败、发送通道无效)，
    ///     则返回包含描述性错误信息的字符串。
    pub async fn send_ws_message(&self, message: WsMessage) -> Result<(), String> {
        // 步骤 1: 检查连接状态
        if !*self.is_connected_status.read().await {
            let err_msg = "客户端未连接到 WebSocket 服务器。在发送消息前请先连接。".to_string();
            error!("WebSocketClientService.send_ws_message: 发送尝试失败: {}", err_msg);
            return Err(err_msg);
        }

        // 步骤 2: 序列化消息为 JSON 字符串
        let message_json = match serde_json::to_string(&message) {
            Ok(json_str) => json_str,
            Err(e) => {
                let err_msg = format!("序列化 WsMessage 到 JSON 失败: {:?}", e);
                error!("WebSocketClientService.send_ws_message: {}", err_msg);
                return Err(err_msg);
            }
        };

        // 步骤 3 & 4: 获取发送通道并发送消息
        let mut sender_guard = self.ws_send_channel.lock().await; // 获取互斥锁
        if let Some(sender) = sender_guard.as_mut() { // 如果发送通道有效
            match sender.send(TungsteniteMessage::Text(message_json)).await { // 异步发送
                Ok(_) => {
                    debug!("WebSocketClientService.send_ws_message: 消息 (类型: {}, ID: {}) 已成功发送到 WebSocket 服务器。", message.message_type, message.message_id);
                    Ok(())
                }
                Err(e) => { // 发送失败，通常表示连接已断开
                    let err_msg = format!(
                        "通过 WebSocket 发送消息 (类型: {}, ID: {}) 失败: {:?}. 可能连接已中断。",
                        message.message_type,
                        message.message_id,
                        e
                    );
                    error!("WebSocketClientService.send_ws_message: {}", err_msg);
                    // 如果发送失败，认为连接已断开，执行完整的断开逻辑。
                    // 这将确保状态被清理，心跳停止，并通知前端。
                    info!("WebSocketClientService.send_ws_message: 由于发送失败，将主动调用 disconnect() 清理连接。");
                    self.disconnect().await.map_err(|disconnect_err| {
                        // 如果 disconnect 本身也返回错误 (虽然当前实现中它总是 Ok(())),
                        // 将其记录下来，但主要返回原始的发送错误。
                        error!("WebSocketClientService.send_ws_message: 在处理发送错误后调用 disconnect() 时也发生错误: {}", disconnect_err);
                        err_msg.clone() // 返回原始错误信息
                    })?;
                    Err(err_msg) // 返回原始的发送错误
                }
            }
        } else { // 发送通道无效 (为 None)
            let err_msg = "WebSocket 发送通道不可用，可能连接正在关闭或已关闭。".to_string();
            error!("WebSocketClientService.send_ws_message: 发送尝试失败: {}", err_msg);
            // 理论上，如果 is_connected_status 是 true，ws_send_channel 不应该是 None。
            // 如果发生这种情况，可能意味着状态不一致，也最好触发一次断开以确保清理。
            info!("WebSocketClientService.send_ws_message: 检测到发送通道为 None 但仍尝试发送，将主动调用 disconnect() 以确保状态一致性。");
            self.disconnect().await.map_err(|disconnect_err| {
                error!("WebSocketClientService.send_ws_message: 在处理发送通道为None后调用 disconnect() 时也发生错误: {}", disconnect_err);
                err_msg.clone()
            })?;
            Err(err_msg)
        }
    }
}

// 确保 Default impl 已被移除
// 如果之前的 edit_file 没有移除它，这里确保它不存在。
// 正确的做法是在 main.rs 的 setup 钩子中使用 WebSocketClientService::new(app_handle) 来创建实例
// 并通过 app.manage() 放入 Tauri State。 