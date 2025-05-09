// rust_websocket_utils/src/client/transport.rs

//! 包含客户端 WebSocket 连接和通信逻辑。

// 旧的 ClientTransport 结构体和实现已被移除或整合。

// use crate::error::WsClientError; // 不再使用，已统一到 WsError
use log::{info, error, debug};
// use tokio::net::TcpStream; // TcpStream 被 ClientWsStream 间接使用
use tokio_tungstenite::{
    connect_async,
    // MaybeTlsStream, // WebSocketStream<MaybeTlsStream<...>> 这样使用即可
    WebSocketStream,
    // tungstenite::http::Response as TungsteniteResponse, // 不再直接使用此类型别名
    tungstenite::protocol::Message, 
    tungstenite::Error as TungsteniteError, 
};
use url::Url;
use crate::error::WsError; // 统一使用 WsError
use crate::message::WsMessage;
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt,
    StreamExt,
};

// pub struct ClientTransport; // 已移除
// impl ClientTransport { ... } // 已移除

pub type ClientWsStream = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// 客户端 WebSocket 连接的处理器
///
/// 封装了与服务器的单个 WebSocket 连接的发送和接收端。
pub struct ClientConnection {
    /// 用于向服务器发送消息的 Sink 端
    pub ws_sender: SplitSink<ClientWsStream, Message>,
    /// 用于从服务器接收消息的 Stream 端 (需要设为 pub 以便测试和外部使用)
    pub ws_receiver: SplitStream<ClientWsStream>,
}

impl ClientConnection {
    /// 向服务器发送一个 WsMessage
    ///
    /// # Arguments
    /// * `message` - 要发送的 `WsMessage` 实例。
    ///
    /// # Returns
    /// * `Result<(), WsError>` - 如果发送成功则返回 Ok，否则返回错误。
    pub async fn send_message(&mut self, message: &WsMessage) -> Result<(), WsError> {
        let msg_json = serde_json::to_string(message)
            .map_err(|e| WsError::SerializationError(e.to_string()))?;
        debug!("客户端发送消息: {}", msg_json);
        self.ws_sender.send(Message::Text(msg_json)).await?;
        Ok(())
    }
}

/// 连接到指定的 WebSocket 服务器
///
/// # Arguments
/// * `url_str` - WebSocket 服务器的 URL 字符串 (例如 "ws://127.0.0.1:8080/ws")。
///
/// # Returns
/// * `Result<ClientConnection, WsError>` - 如果连接和握手成功，则返回 `ClientConnection` 实例，
///   否则返回相应的 `WsError`。
pub async fn connect_client(url_str: String) -> Result<ClientConnection, WsError> {
    info!("客户端：尝试连接到 WebSocket 服务器: {}", url_str);
    let parsed_url = Url::parse(&url_str).map_err(|e| WsError::InvalidUrl(e.to_string()))?;

    match connect_async(parsed_url.as_str()).await {
        Ok((ws_stream, response)) => {
            info!("客户端：成功连接到 {} (HTTP 状态: {})", url_str, response.status());
            debug!("客户端：连接响应头: {:?}", response.headers());
            let (ws_sender, ws_receiver) = ws_stream.split();
            Ok(ClientConnection { ws_sender, ws_receiver })
        }
        Err(e) => {
            error!("客户端：连接到 {} 失败: {}", url_str, e);
            Err(WsError::WebSocketProtocolError(e))
        }
    }
}

/// 从 WebSocket 流中接收并尝试解析一个 WsMessage。
/// 此函数处理单个消息事件，循环读取应由调用方实现。
pub async fn receive_message(
    ws_receiver: &mut SplitStream<ClientWsStream>,
) -> Option<Result<WsMessage, WsError>> {
    // 内部循环仅用于跳过不产生用户级 WsMessage 的控制帧 (Ping, Pong 等)。
    // 主要的消息处理循环应在调用方实现。
    loop {
        match ws_receiver.next().await {
            Some(msg_result) => match msg_result {
                Ok(msg) => match msg {
                    Message::Text(text) => {
                        debug!("客户端收到文本消息: {}", text);
                        break Some(serde_json::from_str::<WsMessage>(&text)
                            .map_err(|e| WsError::DeserializationError(e.to_string())));
                    }
                    Message::Binary(bin) => { 
                        debug!("客户端收到二进制消息: {:?}", bin);
                        break Some(Err(WsError::Message(
                            "收到非预期的二进制消息".to_string(),
                        )));
                    }
                    Message::Ping(ping_data) => { 
                        debug!("客户端收到 Ping: {:?}. 由 tokio-tungstenite 自动处理.", ping_data);
                    }
                    Message::Pong(pong_data) => { 
                        debug!("客户端收到 Pong: {:?}", pong_data);
                    }
                    Message::Close(close_frame) => { 
                        debug!("客户端收到 Close 帧: {:?}", close_frame);
                        break None; 
                    }
                    Message::Frame(_) => { 
                        debug!("客户端收到一个非预期的底层 Frame 类型。正在跳过。");
                    }
                },
                Err(e) => match e {
                    TungsteniteError::ConnectionClosed | TungsteniteError::AlreadyClosed => { 
                        debug!("连接被对方关闭 (在 ws_receiver.next() 期间)。");
                        break None;
                    }
                    _ => {
                        error!("从 WebSocket 流接收消息时发生错误: {}", e);
                        break Some(Err(WsError::WebSocketProtocolError(e)));
                    }
                },
            },
            None => {
                debug!("WebSocket 流已结束 (ws_receiver.next() 返回 None)。");
                break None; 
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::WsMessage;
    use common_models::ws_payloads::EchoPayload; 
    use tokio::time::{timeout, Duration};
    // Import server components needed for the test setup
    use crate::server::transport::{
        start_server as start_echo_server, // 使用我们定义的 start_server 作为回显服务器
        ConnectionHandler as ServerConnectionHandler, 
        receive_message as server_receive_message,
    };
    use futures_util::stream::SplitStream as ServerSplitStream; // 类型别名，如果需要
    use tokio::net::TcpStream as ServerTcpStream; // 服务端 TCP 流类型
    // use tokio_tungstenite::WebSocketStream as ServerWebSocketStream; // 移除未使用的类型别名

    // 辅助函数：启动一个简单的回显服务器，用于客户端测试
    async fn setup_test_echo_server_for_client_tests(addr: String) -> tokio::task::JoinHandle<Result<(), WsError>> {
        tokio::spawn(async move {
            // 使用从 server::transport 导入的 start_server
            start_echo_server(addr, move |mut conn_handler: ServerConnectionHandler, mut server_receiver: ServerSplitStream<WebSocketStream<ServerTcpStream>>| async move {
                info!("[测试回显服务端-客户端测试用] 新客户端已连接。");
                loop {
                    match server_receive_message(&mut server_receiver).await {
                        Some(Ok(ws_msg)) => {
                            info!("[测试回显服务端-客户端测试用] 收到: {:?}, 正在回显。", ws_msg);
                            if let Err(e) = conn_handler.send_message(&ws_msg).await {
                                error!("[测试回显服务端-客户端测试用] 回显消息时出错: {}", e);
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            error!("[测试回显服务端-客户端测试用] 接收消息时出错: {}", e);
                            break;
                        }
                        None => {
                            info!("[测试回显服务端-客户端测试用] 客户端已断开连接。");
                            break;
                        }
                    }
                }
            }).await
        })
    }

    #[tokio::test]
    async fn test_client_connect_send_receive_echo() {
        let _ = env_logger::builder().is_test(true).try_init();
        let server_bind_addr = "127.0.0.1:12346".to_string();
        let client_connect_url = format!("ws://{}", server_bind_addr);

        let server_handle = setup_test_echo_server_for_client_tests(server_bind_addr.clone()).await;
        tokio::time::sleep(Duration::from_millis(200)).await; 

        match connect_client(client_connect_url.clone()).await {
            Ok(mut client_conn) => {
                info!("[测试客户端] 已成功连接到回显服务端。");
                let echo_payload = EchoPayload { content: "来自客户端测试的问候".to_string() };
                let message_to_send = WsMessage::new("ClientEcho".to_string(), &echo_payload)
                    .expect("创建客户端WsMessage失败");

                info!("[测试客户端] 正在发送消息: {:?}", message_to_send);
                if let Err(e) = client_conn.send_message(&message_to_send).await {
                    error!("[测试客户端] 发送消息失败: {}", e);
                    panic!("客户端发送消息失败: {}", e);
                }
                info!("[测试客户端] 消息已发送至回显服务端。");

                info!("[测试客户端] 正在尝试从服务端接收回显...");
                match timeout(Duration::from_secs(5), self::receive_message(&mut client_conn.ws_receiver)).await {
                    Ok(Some(Ok(response_msg))) => {
                        info!("[测试客户端] 收到回显响应: {:?}", response_msg);
                        assert_eq!(response_msg.message_type, "ClientEcho");
                        let received_payload: EchoPayload = response_msg.deserialize_payload()
                            .expect("反序列化回显载荷失败");
                        assert_eq!(received_payload.content, echo_payload.content);
                    }
                    Ok(Some(Err(e))) => panic!("[测试客户端] 从回显服务端接收错误: {}", e),
                    Ok(None) => panic!("[测试客户端] 在接收到回显前连接被服务端关闭。"),
                    Err(e) => panic!("[测试客户端] 等待服务端回显响应超时: {}", e),
                }
            }
            Err(e) => {
                error!("[测试客户端] 连接到回显服务端 {} 失败: {}", client_connect_url, e);
                panic!("客户端连接失败: {}", e);
            }
        }
        server_handle.abort(); 
    }
} 