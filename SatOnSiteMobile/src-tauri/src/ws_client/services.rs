// SatOnSiteMobile/src-tauri/src/ws_client/services.rs
// 内容与 SatControlCenter/src-tauri/src/ws_client/services.rs 基本相同，
// 主要区别在于 ClientRole 的设置 (如果适用) 和日志中的应用名称标识。
// WsService 的核心连接、消息处理、事件发射逻辑是共享的。

use common_models::{
    enums::ClientRole,
    task_state::TaskDebugState,
    ws_messages::WsMessage,
    ws_payloads::{
        PartnerStatusPayload, RegisterResponsePayload, 
        PARTNER_STATUS_UPDATE_MESSAGE_TYPE, REGISTER_RESPONSE_MESSAGE_TYPE,
        TASK_STATE_UPDATE_MESSAGE_TYPE,
    },
};
// 引用 SatOnSiteMobile 自己的 events 模块
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
use tauri::{AppHandle, Manager, Wry}; 
use tokio::{
    net::TcpStream,
    sync::{mpsc, Mutex, RwLock}, 
};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use url::Url;

#[derive(Debug)]
pub enum WsRequest {
    SendMessage(WsMessage),
}

pub struct WsService {
    app_handle: AppHandle<Wry>,
    ws_url: Url,
    current_group_id: Arc<RwLock<Option<String>>>,
    current_task_id: Arc<RwLock<Option<String>>>,
    local_task_state: Arc<RwLock<Option<TaskDebugState>>>,
    internal_request_tx: Arc<Mutex<Option<mpsc::Sender<WsRequest>>>>,
    ws_write_sink: Arc<Mutex<Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tokio_tungstenite::tungstenite::Message>>>>,
}

impl WsService {
    pub fn new(app_handle: AppHandle<Wry>, ws_url_str: &str) -> Self {
        info!("WsService (SatOnSiteMobile) 初始化，目标 WebSocket URL: {}", ws_url_str);
        let url = Url::parse(ws_url_str).expect("无效的 WebSocket URL");
        Self {
            app_handle,
            ws_url: url,
            current_group_id: Arc::new(RwLock::new(None)),
            current_task_id: Arc::new(RwLock::new(None)),
            local_task_state: Arc::new(RwLock::new(None)),
            internal_request_tx: Arc::new(Mutex::new(None)),
            ws_write_sink: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn submit_request(&self, request: WsRequest) -> Result<(), String> {
        debug!("WsService (SatOnSiteMobile) 收到外部请求: {:?}", request);
        let tx_guard = self.internal_request_tx.lock().await;
        if let Some(tx) = tx_guard.as_ref() {
            if let Err(e) = tx.send(request).await {
                error!("通过 mpsc channel 发送 WsRequest (SatOnSiteMobile) 失败: {}", e);
                Err(format!("发送 WebSocket 内部请求失败: {}", e))
            } else {
                debug!("WsRequest (SatOnSiteMobile) 已成功发送到内部处理循环");
                Ok(())
            }
        } else {
            error!("WebSocket 内部请求发送端 (SatOnSiteMobile) 未初始化。服务可能未连接。");
            Err("WebSocket 服务未连接或内部发送端未初始化".to_string())
        }
    }

    async fn send_ws_message_via_sink(&self, ws_message: WsMessage) -> Result<(), String> {
        let json_string = match serde_json::to_string(&ws_message) {
            Ok(s) => s,
            Err(e) => {
                error!("序列化 WsMessage (SatOnSiteMobile) 失败: {:?}", e);
                return Err(format!("序列化 WsMessage 失败: {}", e));
            }
        };

        let mut sink_guard = self.ws_write_sink.lock().await;
        if let Some(sink) = sink_guard.as_mut() {
            info!("准备通过 WebSocket Sink (SatOnSiteMobile) 发送消息 (前100字节): {}...", &json_string[0..std::cmp::min(json_string.len(), 100)]);
            if let Err(e) = sink.send(tokio_tungstenite::tungstenite::Message::Text(json_string)).await {
                error!("通过 WebSocket Sink (SatOnSiteMobile) 发送消息失败: {}", e);
                Err(format!("WebSocket Sink 发送失败: {}", e))
            } else {
                debug!("WebSocket 消息通过 Sink (SatOnSiteMobile) 发送成功");
                Ok(())
            }
        } else {
            error!("WebSocket Sink (SatOnSiteMobile) 未初始化，无法发送消息");
            Err("WebSocket Sink 未初始化，可能连接已断开".to_string())
        }
    }

    pub async fn run(self: Arc<Self>) {
        let (internal_tx, internal_rx) = mpsc::channel::<WsRequest>(32);
        *self.internal_request_tx.lock().await = Some(internal_tx);
        info!("WsService (SatOnSiteMobile) 主循环已启动，内部请求发送端已设置。");

        let service_for_sender = Arc::clone(&self);
        tokio::spawn(async move {
            service_for_sender.outgoing_requests_processor(internal_rx).await;
        });
        info!("已启动独立的 outgoing_requests_processor 任务 (SatOnSiteMobile)。");

        loop {
            info!("尝试连接到 WebSocket 服务器 (SatOnSiteMobile): {}", self.ws_url);
            emit_tauri_event(&self.app_handle, "event://ws-connection-status", 
                serde_json::json!({"status": "connecting", "url": self.ws_url.to_string(), "client": "SatOnSiteMobile"}));

            match connect_async(self.ws_url.clone()).await {
                Ok((ws_stream, response)) => {
                    info!("成功连接到 WebSocket 服务器 (SatOnSiteMobile): {}. 响应状态: {:?}", self.ws_url, response.status());
                    emit_tauri_event(&self.app_handle, "event://ws-connection-status", 
                        serde_json::json!({"status": "connected", "url": self.ws_url.to_string(), "client": "SatOnSiteMobile"}));

                    let (write_sink, read_stream) = ws_stream.split();
                    *self.ws_write_sink.lock().await = Some(write_sink);
                    info!("WebSocket 读写流已分离 (SatOnSiteMobile)。");

                    let service_for_reader = Arc::clone(&self);
                    let mut incoming_messages_task = tokio::spawn(async move {
                        service_for_reader.handle_incoming_messages(read_stream).await;
                    });
                    info!("已启动独立的 handle_incoming_messages 任务 (SatOnSiteMobile)。");
                    
                    let _ = (&mut incoming_messages_task).await;
                    info!("handle_incoming_messages 任务已终止 (SatOnSiteMobile)。");
                    
                    *self.ws_write_sink.lock().await = None;
                    warn!("WebSocket 连接已断开或读取任务终止 (SatOnSiteMobile)。");
                    emit_tauri_event(&self.app_handle, "event://ws-connection-status", 
                        serde_json::json!({"status": "disconnected", "url": self.ws_url.to_string(), "client": "SatOnSiteMobile"}));
                }
                Err(e) => {
                    error!("连接 WebSocket 服务器失败 (SatOnSiteMobile): {}.", e);
                    emit_tauri_event(&self.app_handle, "event://ws-connection-status", 
                        serde_json::json!({"status": "error", "message": e.to_string(), "client": "SatOnSiteMobile"}));
                }
            }
            info!("等待5秒后尝试重连 WebSocket (SatOnSiteMobile)...");
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }
    
    async fn outgoing_requests_processor(self: Arc<Self>, mut receiver: mpsc::Receiver<WsRequest>) {
        info!("WebSocket 发送请求处理器 (SatOnSiteMobile) 已启动。");
        while let Some(request) = receiver.recv().await {
            debug!("发送处理器 (SatOnSiteMobile) 收到 WsRequest: {:?}", request);
            match request {
                WsRequest::SendMessage(ws_message) => {
                    if let Err(e) = self.send_ws_message_via_sink(ws_message).await {
                        error!("从 outgoing_requests_processor (SatOnSiteMobile) 发送 WebSocket 消息失败: {}", e);
                    }
                }
            }
        }
        info!("WebSocket 发送请求处理器 (SatOnSiteMobile) 已关闭。");
    }

    async fn handle_incoming_messages(
        &self, 
        mut read_stream: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    ) {
        info!("开始监听来自 WebSocket 服务器的传入消息 (SatOnSiteMobile)...");
        while let Some(message_result) = read_stream.next().await {
            match message_result {
                Ok(msg) => {
                    match msg {
                        tokio_tungstenite::tungstenite::Message::Text(text) => {
                            debug!("从服务器收到文本消息 (SatOnSiteMobile, 长度 {}): {}...", text.len(), &text[0..std::cmp::min(text.len(),100)]);
                            self.process_received_text_message(&text).await;
                        }
                        _ => { /* 其他消息类型处理与 SatControlCenter 一致 */ }
                    }
                }
                Err(e) => {
                    error!("读取 WebSocket 消息时发生错误 (SatOnSiteMobile): {}. 中断消息处理。", e);
                    break; 
                }
            }
        }
        info!("WebSocket 消息接收循环已终止 (SatOnSiteMobile)。");
    }

    async fn process_received_text_message(&self, text_message: &str) {
        match serde_json::from_str::<WsMessage>(text_message) {
            Ok(ws_msg) => {
                debug!("成功解析 WsMessage (SatOnSiteMobile): 类型 '{}', Payload (前100字节): {}...", ws_msg.message_type, &ws_msg.payload[0..std::cmp::min(ws_msg.payload.len(),100)]);
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
                        warn!("收到未知的已解析 WebSocket 消息类型 (SatOnSiteMobile): '{}'", unknown_type);
                    }
                }
            }
            Err(e) => {
                error!(
                    "反序列化收到的文本消息为 WsMessage (SatOnSiteMobile) 失败: {}. 原始消息 (前200字节): {}...",
                    e, &text_message[0..std::cmp::min(text_message.len(),200)]
                );
            }
        }
    }

    async fn handle_register_response_payload(&self, payload_str: &str) {
        match serde_json::from_str::<RegisterResponsePayload>(payload_str) {
            Ok(payload) => {
                info!(
                    "处理 'RegisterResponse' (SatOnSiteMobile): success={}, group_id={:?}, role={:?}, msg='{}'",
                    payload.success, payload.effective_group_id, payload.effective_role, payload.message.as_deref().unwrap_or("")
                );
                let task_id_for_event = self.current_task_id.read().await.clone();
                if payload.success {
                    if let Some(group_id) = payload.effective_group_id.as_ref() {
                        *self.current_group_id.write().await = Some(group_id.clone());
                        info!("客户端 (SatOnSiteMobile) 成功注册到组 '{}'.", group_id);
                    }
                }
                let event_payload = WsRegistrationStatusEvent {
                    success: payload.success,
                    message: payload.message.clone(),
                    group_id: payload.effective_group_id.clone(),
                    task_id: task_id_for_event,
                };
                emit_tauri_event(&self.app_handle, EVENT_WS_REGISTRATION_STATUS, event_payload);
            }
            Err(e) => error!("反序列化 'RegisterResponse' Payload (SatOnSiteMobile) 失败: {}. Payload: {}", e, payload_str),
        }
    }

    async fn handle_partner_status_update_payload(&self, payload_str: &str) {
        match serde_json::from_str::<PartnerStatusPayload>(payload_str) {
            Ok(payload) => {
                info!("处理 'PartnerStatusUpdate' (SatOnSiteMobile): partner_role={:?}, is_online={}, group_id={}", payload.partner_role, payload.is_online, payload.group_id);
                let event_payload = WsPartnerStatusEvent { /* ... */ partner_role: payload.partner_role, is_online: payload.is_online, group_id: payload.group_id };
                emit_tauri_event(&self.app_handle, EVENT_WS_PARTNER_STATUS, event_payload);
            }
            Err(e) => error!("反序列化 'PartnerStatusUpdate' Payload (SatOnSiteMobile) 失败: {}. Payload: {}", e, payload_str),
        }
    }

    async fn handle_task_state_update_payload(&self, payload_str: &str) {
        match serde_json::from_str::<TaskDebugState>(payload_str) {
            Ok(new_state) => {
                info!("处理 'TaskStateUpdate' (SatOnSiteMobile): task_id={}, group_id={}", new_state.task_id, new_state.group_id);
                *self.local_task_state.write().await = Some(new_state.clone());
                info!("本地 TaskDebugState (SatOnSiteMobile) 已更新: task_id: {}", new_state.task_id);
                *self.current_group_id.write().await = Some(new_state.group_id.clone());
                *self.current_task_id.write().await = Some(new_state.task_id.clone());
                let event_payload = LocalTaskStateUpdatedEvent { new_state };
                emit_tauri_event(&self.app_handle, EVENT_LOCAL_TASK_STATE_UPDATED, event_payload);
            }
            Err(e) => error!("反序列化 'TaskStateUpdate' Payload (SatOnSiteMobile) 失败: {}. Payload: {}", e, payload_str),
        }
    }

    pub async fn set_current_task_context(&self, _group_id: String, task_id: String) {
        // For SatOnSiteMobile, group_id is primarily determined by RegisterResponse.
        // Task_id is set from the command context.
        *self.current_task_id.write().await = Some(task_id.clone());
        info!("WsService (SatOnSiteMobile) 上下文已设置: current_task_id={} (group_id 将由注册响应确定)", task_id);
    }
}

pub struct AppState {
    pub ws_service: Arc<WsService>,
}

pub fn setup_and_start_ws_service(app_handle: AppHandle<Wry>, ws_url: String) -> Arc<WsService> {
    info!("开始设置和启动 WsService (SatOnSiteMobile)...");
    let ws_service_instance = Arc::new(WsService::new(app_handle.clone(), &ws_url));
    let service_to_run = Arc::clone(&ws_service_instance);
    tokio::spawn(async move {
        service_to_run.run().await;
        warn!("WsService 的 run 循环已退出 (SatOnSiteMobile)。");
    });
    info!("WsService 实例已创建并已安排运行 (SatOnSiteMobile)。");
    ws_service_instance
} 