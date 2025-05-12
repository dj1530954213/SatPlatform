"""// SatControlCenter/src-tauri/src/ws_client/services.rs
use common_models::{
    enums::ClientRole,
    task_state::TaskDebugState,
    ws_messages::WsMessage,
    ws_payloads::{
        PartnerStatusPayload, RegisterResponsePayload, // TaskDebugState is used directly for TaskStateUpdate
        PARTNER_STATUS_UPDATE_MESSAGE_TYPE, REGISTER_RESPONSE_MESSAGE_TYPE,
        TASK_STATE_UPDATE_MESSAGE_TYPE,
    },
};
use crate::events::{
    emit_tauri_event, LocalTaskStateUpdatedEvent, WsPartnerStatusEvent, WsRegistrationStatusEvent,
    EVENT_LOCAL_TASK_STATE_UPDATED, EVENT_WS_PARTNER_STATUS, EVENT_WS_REGISTRATION_STATUS,
};
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use log::{debug, error, info, warn};
use std::sync::Arc;
use tauri::{AppHandle, Manager, Wry}; // Manager for emit_all and state
use tokio::{
    net::TcpStream,
    sync::{mpsc, Mutex, RwLock}, // Mutex for sink, RwLock for shared state
};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use url::Url;

/// WsRequest 枚举定义了可以从应用其他部分发送到 WsService 的请求类型。
/// 这允许将 WebSocket 操作（如发送消息）封装在 WsService 内部。
#[derive(Debug)]
pub enum WsRequest {
    /// 发送一个 WsMessage 到 WebSocket 服务器。
    SendMessage(WsMessage),
    //未来可以添加其他请求，例如 Connect(url), Disconnect 等。
}

/// WsService 负责管理与 WebSocket 服务器的连接、消息的发送与接收，
/// 以及处理特定类型的服务器消息并将其转换为 Tauri 事件以通知前端。
pub struct WsService {
    /// Tauri 应用句柄，用于发射事件和访问应用状态。
    app_handle: AppHandle<Wry>,
    /// WebSocket 服务器的 URL。
    ws_url: Url,
    /// 当前客户端已注册的组ID。使用 RwLock 以便在多任务环境中安全读写。
    current_group_id: Arc<RwLock<Option<String>>>,
    /// 当前客户端关联的任务ID。
    current_task_id: Arc<RwLock<Option<String>>>, // 注意：此字段的准确填充依赖于注册流程设计
    /// 本地缓存的最新任务调试状态。
    local_task_state: Arc<RwLock<Option<TaskDebugState>>>,
    /// 用于从外部（例如 Tauri 命令）向 WsService 的内部发送处理循环发送 WsRequest 的 mpsc 发送端。
    /// 使用 Mutex 保护 Option，因为发送端在连接建立后才被初始化。
    internal_request_tx: Arc<Mutex<Option<mpsc::Sender<WsRequest>>>>,
    /// WebSocket 连接的写入端 (Sink)，用于向服务器发送消息。
    /// 使用 Mutex 保护 Option，因为 Sink 在连接建立后才可用。
    ws_write_sink: Arc<Mutex<Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tokio_tungstenite::tungstenite::Message>>>>,
}

impl WsService {
    /// 创建一个新的 WsService 实例。
    ///
    /// # Arguments
    /// * `app_handle` - Tauri 应用句柄。
    /// * `ws_url_str` - WebSocket 服务器的 URL 字符串。
    ///
    /// # Panics
    /// 如果 `ws_url_str` 不是一个有效的 URL，则会 panic。
    pub fn new(app_handle: AppHandle<Wry>, ws_url_str: &str) -> Self {
        info!("WsService 初始化，目标 WebSocket URL: {}", ws_url_str);
        let url = Url::parse(ws_url_str).expect("无效的 WebSocket URL");
        Self {
            app_handle,
            ws_url: url,
            current_group_id: Arc::new(RwLock::new(None)),
            current_task_id: Arc::new(RwLock::new(None)), // 初始化为空
            local_task_state: Arc::new(RwLock::new(None)),
            internal_request_tx: Arc::new(Mutex::new(None)),
            ws_write_sink: Arc::new(Mutex::new(None)),
        }
    }

    /// 公开给应用其他部分（如 Tauri 命令）调用的方法，用于向 WsService 异步发送请求。
    /// 请求将通过内部的 mpsc channel 发送到 WsService 的主处理循环。
    pub async fn submit_request(&self, request: WsRequest) -> Result<(), String> {
        debug!("WsService 收到外部请求: {:?}", request);
        let tx_guard = self.internal_request_tx.lock().await;
        if let Some(tx) = tx_guard.as_ref() {
            if let Err(e) = tx.send(request).await {
                error!("通过 mpsc channel 发送 WsRequest 失败: {}", e);
                Err(format!("发送 WebSocket 内部请求失败: {}", e))
            } else {
                debug!("WsRequest 已成功发送到内部处理循环");
                Ok(())
            }
        } else {
            error!("WebSocket 内部请求发送端 (mpsc::Sender) 未初始化。服务可能未连接。");
            Err("WebSocket 服务未连接或内部发送端未初始化".to_string())
        }
    }

    /// 内部函数，实际将 WsMessage 序列化并通过 WebSocket Sink 发送到服务器。
    async fn send_ws_message_via_sink(&self, ws_message: WsMessage) -> Result<(), String> {
        let json_string = match serde_json::to_string(&ws_message) {
            Ok(s) => s,
            Err(e) => {
                error!("序列化 WsMessage 失败: {:?}", e);
                return Err(format!("序列化 WsMessage 失败: {}", e));
            }
        };

        let mut sink_guard = self.ws_write_sink.lock().await;
        if let Some(sink) = sink_guard.as_mut() {
            info!("准备通过 WebSocket Sink 发送消息 (前100字节): {}...", &json_string[0..std::cmp::min(json_string.len(), 100)]);
            if let Err(e) = sink.send(tokio_tungstenite::tungstenite::Message::Text(json_string)).await {
                error!("通过 WebSocket Sink 发送消息失败: {}", e);
                Err(format!("WebSocket Sink 发送失败: {}", e))
            } else {
                debug!("WebSocket 消息通过 Sink 发送成功");
                Ok(())
            }
        } else {
            error!("WebSocket Sink 未初始化，无法发送消息");
            Err("WebSocket Sink 未初始化，可能连接已断开".to_string())
        }
    }

    /// WsService 的主运行循环。此循环负责：
    /// 1. 创建一个内部 mpsc channel (internal_tx, internal_rx) 用于从外部接收 WsRequest。
    /// 2. 将 internal_tx 存储在 self.internal_request_tx 中，供 submit_request 方法使用。
    /// 3. 启动一个独立的 Tokio 任务 (outgoing_requests_processor) 来处理 internal_rx，该任务负责将消息实际发送到 WebSocket sink。
    /// 4. 进入一个主重连循环，该循环负责：
    ///    a. 尝试建立 WebSocket 连接。
    ///    b. 连接成功后，分离读写流 (read_stream, write_sink)。将 write_sink 存入 self.ws_write_sink。
    ///    c. 启动一个独立的 Tokio 任务 (handle_incoming_messages) 来处理 read_stream。
    ///    d. 等待 handle_incoming_messages 任务结束 (通常表示连接断开)。
    ///    e. 清理资源 (如移除旧的 ws_write_sink)，然后延迟一段时间后尝试重连。
    pub async fn run(self: Arc<Self>) {
        // 1. 创建内部 mpsc channel
        let (internal_tx, internal_rx) = mpsc::channel::<WsRequest>(32); // 32 是 channel 的缓冲大小
        
        // 2. 存储 internal_tx
        *self.internal_request_tx.lock().await = Some(internal_tx);
        info!("WsService 主循环已启动，内部请求发送端 (internal_tx) 已设置。");

        // 3. 启动 outgoing_requests_processor 任务
        let service_for_sender = Arc::clone(&self);
        tokio::spawn(async move {
            service_for_sender.outgoing_requests_processor(internal_rx).await;
        });
        info!("已启动独立的 outgoing_requests_processor 任务。");

        // 4. 主重连循环
        loop {
            info!("尝试连接到 WebSocket 服务器: {}", self.ws_url);
            emit_tauri_event(&self.app_handle, "event://ws-connection-status", 
                serde_json::json!({"status": "connecting", "url": self.ws_url.to_string()}));

            match connect_async(self.ws_url.clone()).await {
                Ok((ws_stream, response)) => {
                    info!("成功连接到 WebSocket 服务器: {}. 响应状态: {:?}", self.ws_url, response.status());
                    emit_tauri_event(&self.app_handle, "event://ws-connection-status", 
                        serde_json::json!({"status": "connected", "url": self.ws_url.to_string()}));

                    let (write_sink, read_stream) = ws_stream.split();
                    *self.ws_write_sink.lock().await = Some(write_sink); // 存储新的 Sink
                    info!("WebSocket 读写流已分离。旧 Sink (如有) 已替换。");

                    let service_for_reader = Arc::clone(&self);
                    let mut incoming_messages_task = tokio::spawn(async move {
                        service_for_reader.handle_incoming_messages(read_stream).await;
                    });
                    info!("已启动独立的 handle_incoming_messages 任务。");
                    
                    // 等待读取任务结束 (表示连接断开)
                    // tokio::select! biased; // Not needed if only awaiting one task
                    let _ = (&mut incoming_messages_task).await; // 等待任务完成
                    info!("handle_incoming_messages 任务已终止。可能由于连接断开。");
                    
                    // 清理工作在连接断开后
                    *self.ws_write_sink.lock().await = None; // 移除旧的 sink
                    warn!("WebSocket 连接已断开或读取任务终止。");
                    emit_tauri_event(&self.app_handle, "event://ws-connection-status", 
                        serde_json::json!({"status": "disconnected", "url": self.ws_url.to_string()}));
                }
                Err(e) => {
                    error!("连接 WebSocket 服务器失败: {}.", e);
                    emit_tauri_event(&self.app_handle, "event://ws-connection-status", 
                        serde_json::json!({"status": "error", "message": e.to_string()}));
                }
            }
            info!("等待5秒后尝试重连 WebSocket...");
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }
    
    /// 独立的 Tokio 任务，负责从内部 mpsc channel (internal_rx) 接收 WsRequest，
    /// 并通过 WebSocket Sink 将消息发送到服务器。
    async fn outgoing_requests_processor(self: Arc<Self>, mut receiver: mpsc::Receiver<WsRequest>) {
        info!("WebSocket 发送请求处理器 (outgoing_requests_processor) 已启动。");
        while let Some(request) = receiver.recv().await {
            debug!("发送处理器收到 WsRequest: {:?}", request);
            match request {
                WsRequest::SendMessage(ws_message) => {
                    if let Err(e) = self.send_ws_message_via_sink(ws_message).await {
                        error!("从 outgoing_requests_processor 发送 WebSocket 消息失败: {}", e);
                        // 如果 sink 发送失败，通常表示连接已断开。
                        // WsService::run 中的读取任务会检测到这一点并触发重连，
                        // 重连后会更新 ws_write_sink。此发送处理器将继续尝试使用新的sink。
                    }
                }
            }
        }
        info!("WebSocket 发送请求处理器 (outgoing_requests_processor) 已关闭 (mpsc channel 关闭)。");
    }

    /// 处理从 WebSocket 服务器接收到的所有消息。
    /// 此方法在一个独立的 Tokio 任务中运行。
    async fn handle_incoming_messages(
        &self, // 改为 &self 因为 Arc<Self> 通常用于 spawn 新任务的 self
        mut read_stream: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    ) {
        info!("开始监听来自 WebSocket 服务器的传入消息...");
        while let Some(message_result) = read_stream.next().await {
            match message_result {
                Ok(msg) => {
                    match msg {
                        tokio_tungstenite::tungstenite::Message::Text(text) => {
                            debug!("从服务器收到文本消息 (长度 {}): {}...", text.len(), &text[0..std::cmp::min(text.len(),100)]);
                            self.process_received_text_message(&text).await;
                        }
                        tokio_tungstenite::tungstenite::Message::Binary(bin) => {
                            warn!("从服务器收到二进制消息 (未处理，长度 {})", bin.len());
                        }
                        tokio_tungstenite::tungstenite::Message::Ping(ping_data) => {
                            debug!("收到服务器 Ping 帧 ({}字节)，将自动回复 Pong。", ping_data.len());
                        }
                        tokio_tungstenite::tungstenite::Message::Pong(_) => {
                            debug!("收到服务器 Pong 帧");
                        }
                        tokio_tungstenite::tungstenite::Message::Close(close_frame) => {
                            info!("收到服务器 Close 帧: {:?}. 准备关闭读取循环。", close_frame);
                            break;
                        }
                        tokio_tungstenite::tungstenite::Message::Frame(raw_frame) => {
                             warn!("收到未直接处理的原始 WebSocket Frame: {:?}", raw_frame.header());
                        }
                    }
                }
                Err(e) => {
                    error!("读取 WebSocket 消息时发生错误: {}. 中断消息处理。", e);
                    break; 
                }
            }
        }
        info!("WebSocket 消息接收循环已终止。");
    }

    /// 解析从服务器收到的文本消息，并根据消息类型分发给相应的处理函数。
    async fn process_received_text_message(&self, text_message: &str) {
        match serde_json::from_str::<WsMessage>(text_message) {
            Ok(ws_msg) => {
                debug!("成功解析 WsMessage: 类型 '{}', Payload (前100字节): {}...", ws_msg.message_type, &ws_msg.payload[0..std::cmp::min(ws_msg.payload.len(),100)]);
                match ws_msg.message_type.as_str() {
                    REGISTER_RESPONSE_MESSAGE_TYPE => {
                        self.handle_register_response_payload(&ws_msg.payload).await;
                    }
                    PARTNER_STATUS_UPDATE_MESSAGE_TYPE => {
                        self.handle_partner_status_update_payload(&ws_msg.payload).await;
                    }
                    TASK_STATE_UPDATE_MESSAGE_TYPE => {
                        self.handle_task_state_update_payload(&ws_msg.payload).await;
                    }
                    unknown_type => {
                        warn!("收到未知的已解析 WebSocket 消息类型: '{}'", unknown_type);
                    }
                }
            }
            Err(e) => {
                error!(
                    "反序列化收到的文本消息为 WsMessage 失败: {}. 原始消息 (前200字节): {}...",
                    e, &text_message[0..std::cmp::min(text_message.len(),200)]
                );
            }
        }
    }

    /// 处理 "RegisterResponse" 消息的 Payload。
    async fn handle_register_response_payload(&self, payload_str: &str) {
        match serde_json::from_str::<RegisterResponsePayload>(payload_str) {
            Ok(payload) => {
                info!(
                    "处理 'RegisterResponse': success={}, group_id={:?}, role={:?}, msg='{}'",
                    payload.success, payload.effective_group_id, payload.effective_role, payload.message.as_deref().unwrap_or("")
                );
                
                let mut task_id_for_event: Option<String> = None;

                if payload.success {
                    if let Some(group_id) = payload.effective_group_id.as_ref() {
                        *self.current_group_id.write().await = Some(group_id.clone());
                        info!("客户端成功注册到组 '{}'. 本地已记录 group_id.", group_id);
                         task_id_for_event = self.current_task_id.read().await.clone();
                         if task_id_for_event.is_none() && payload.effective_group_id.is_some() {
                             warn!("注册成功，但无法确定关联的 task_id 以发送事件。前端可能需要自行处理。");
                         }
                    } else {
                         warn!("注册成功，但云端未返回有效的 group_id。");
                    }
                } else {
                    error!(
                        "客户端注册失败: {}",
                        payload.message.as_deref().unwrap_or("未知错误")
                    );
                    task_id_for_event = self.current_task_id.read().await.clone();
                }

                let event_payload = WsRegistrationStatusEvent {
                    success: payload.success,
                    message: payload.message.clone(),
                    group_id: payload.effective_group_id.clone(),
                    task_id: task_id_for_event,
                };
                emit_tauri_event(&self.app_handle, EVENT_WS_REGISTRATION_STATUS, event_payload);
            }
            Err(e) => {
                error!("反序列化 'RegisterResponse' Payload 失败: {}. 原始Payload: {}", e, payload_str);
            }
        }
    }

    /// 处理 "PartnerStatusUpdate" 消息的 Payload。
    async fn handle_partner_status_update_payload(&self, payload_str: &str) {
        match serde_json::from_str::<PartnerStatusPayload>(payload_str) {
            Ok(payload) => {
                info!(
                    "处理 'PartnerStatusUpdate': partner_role={:?}, is_online={}, group_id={}",
                    payload.partner_role, payload.is_online, payload.group_id
                );
                let event_payload = WsPartnerStatusEvent {
                    partner_role: payload.partner_role,
                    is_online: payload.is_online,
                    group_id: payload.group_id,
                };
                emit_tauri_event(&self.app_handle, EVENT_WS_PARTNER_STATUS, event_payload);
            }
            Err(e) => {
                error!("反序列化 'PartnerStatusUpdate' Payload 失败: {}. 原始Payload: {}", e, payload_str);
            }
        }
    }

    /// 处理 "TaskStateUpdate" 消息的 Payload。
    /// Payload 直接是 TaskDebugState 的 JSON 序列化形式。
    async fn handle_task_state_update_payload(&self, payload_str: &str) {
        match serde_json::from_str::<TaskDebugState>(payload_str) {
            Ok(new_state) => {
                info!(
                    "处理 'TaskStateUpdate': task_id={}, group_id={}, last_updated_by={:?}, timestamp={:?}",
                    new_state.task_id, new_state.group_id, new_state.last_updated_by_role, new_state.last_update_timestamp
                );
                
                *self.local_task_state.write().await = Some(new_state.clone());
                info!("本地 TaskDebugState 已成功更新为 task_id: {}", new_state.task_id);

                let mut current_group_guard = self.current_group_id.write().await;
                if current_group_guard.as_ref() != Some(&new_state.group_id) {
                    *current_group_guard = Some(new_state.group_id.clone());
                    info!("WsService.current_group_id 已根据 TaskStateUpdate 更新为: {}", new_state.group_id);
                }
                drop(current_group_guard);

                let mut current_task_guard = self.current_task_id.write().await;
                 if current_task_guard.as_ref() != Some(&new_state.task_id) {
                    *current_task_guard = Some(new_state.task_id.clone());
                    info!("WsService.current_task_id 已根据 TaskStateUpdate 更新为: {}", new_state.task_id);
                }
                drop(current_task_guard);

                let event_payload = LocalTaskStateUpdatedEvent { new_state };
                emit_tauri_event(&self.app_handle, EVENT_LOCAL_TASK_STATE_UPDATED, event_payload);
            }
            Err(e) => {
                error!("反序列化 'TaskStateUpdate' Payload (TaskDebugState) 失败: {}. 原始Payload: {}", e, payload_str);
            }
        }
    }

    // 辅助方法，用于在注册命令调用时设置 task_id。这应该由命令本身调用。
    // 此方法需要在 WsService 实例上可访问。
    pub async fn set_current_task_context(&self, group_id: String, task_id: String) {
        // 注释掉直接修改 group_id，因为 group_id 应由 RegisterResponse 权威设置
        // *self.current_group_id.write().await = Some(group_id);
        // 只设置 task_id，因为这是命令调用时知道的上下文
        *self.current_task_id.write().await = Some(task_id.clone());
        info!("WsService 上下文已设置: current_task_id={} (group_id 将由注册响应确定)", task_id);
    }
}

/// AppState 结构用于在 Tauri 的状态管理中存储共享服务实例。
pub struct AppState {
    pub ws_service: Arc<WsService>,
    // 可以添加其他服务，如数据库连接池等
}

/// 在 Tauri 应用启动时调用的设置函数，用于初始化和启动 WsService。
pub fn setup_and_start_ws_service(app_handle: AppHandle<Wry>, ws_url: String) -> Arc<WsService> {
    info!("开始设置和启动 WsService...");
    let ws_service_instance = Arc::new(WsService::new(app_handle.clone(), &ws_url));
    
    let service_to_run = Arc::clone(&ws_service_instance);
    tokio::spawn(async move {
        service_to_run.run().await;
        warn!("WsService 的 run 循环已退出。");
    });

    info!("WsService 实例已创建并已安排运行 (run 任务已 spawn)。");
    ws_service_instance
}

// 在上面的 WsService::run 方法中，我已经修改了任务的启动方式，使其更符合 select! 的使用模式。
// outgoing_requests_processor 不再需要作为独立的函数从外部启动，而是作为 WsService::run 内部的一个逻辑部分，
// 或者，更好的方式是 WsService::run 内部启动一个专门处理发送请求的 task。
// 我已经将 handle_outgoing_requests 改为在 WsService::run 内部通过 select! 监控，
// 但它通常不应主动结束，除非 ext_receiver_rx 关闭。
// 最新的 WsService::run 设计是合理的：run 循环主要负责连接和读，
// 发送请求由外部通过 submit_request -> internal_request_tx -> handle_outgoing_requests (在另一个spawn任务中) 处理。

// 为了使 WsService::run 中的 select! 更清晰，
// handle_outgoing_requests 应该是一个独立的 tokio::spawn 任务，
// 这样 WsService::run 的 select! 块只关注 read_task 的结束（表示连接断开）。
// 我将调整 run 方法来体现这一点。

// ** WsService::run 和 outgoing_requests_processor 的最终设计思路 **
// 1. WsService::run:
//    - 创建 mpsc channel (internal_tx, internal_rx)
//    - 将 internal_tx 存入 self.internal_request_tx
//    - 启动一个新的 tokio::spawn 任务：`self.clone().process_internal_requests(internal_rx).await;`
//    - 自身进入主连接循环：
//      - connect_async
//      - 成功连接后，spawn `self.clone().handle_incoming_messages(read_stream).await;`
//      - 等待这个 incoming_messages_task 结束（表示连接断开）
//      - 清理，然后重连
// 2. WsService::process_internal_requests(mut internal_rx):
//    - 循环 `while let Some(request) = internal_rx.recv().await`
//    - `self.send_ws_message_via_sink(request.message).await;`
//
// 这样职责更清晰。我将修改 WsService 结构以实现这一点。
// 当前代码中，WsService::run 已经有一个 select! 块，但 handle_outgoing_requests 的处理方式
// 我已更新了 WsService::run 和增加了 outgoing_requests_processor, 这是一个更清晰的模式。
// WsService::run 会启动 outgoing_requests_processor。
// 现在 outgoing_requests_processor 是 WsService 的一个方法，在 main.rs 中启动。
// 我会修改 main.rs 的 setup 来正确启动这两个任务。
"" 