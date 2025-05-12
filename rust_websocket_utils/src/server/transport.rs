// rust_websocket_utils/src/server/transport.rs

//! 包含服务端 WebSocket 监听、接受连接和通信逻辑。

use crate::error::WsError;
use crate::message::WsMessage;
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt,
    StreamExt,
};
use log::{debug, error, info};
// use std::net::SocketAddr; // 已注释
use tokio::net::TcpStream; // 只导入 TcpStream，因为 TcpListener 是通过完整路径使用的
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        handshake::server::{Request, Response, ErrorResponse},
        http::HeaderValue,
        protocol::Message,
        Error as TungsteniteError
    },
    WebSocketStream,
};

/// `WsStream` 是一个类型别名，代表经过 WebSocket 握手后的 TCP 流。
// pub type WsStream = WebSocketStream<TcpStream>; // 这个类型别名主要被旧的ServerTransport使用

/// 服务端单个连接的处理器
///
/// 每个新的 WebSocket 连接都会在一个新的 Tokio 任务中运行此函数。
/// 它负责处理来自该连接的消息接收、分发以及向该连接发送消息。
pub struct ConnectionHandler {
    ws_sender: SplitSink<WebSocketStream<TcpStream>, Message>,
}

impl ConnectionHandler {
    /// 发送一个 WsMessage 到客户端
    pub async fn send_message(&mut self, message: &WsMessage) -> Result<(), WsError> {
        let msg_json = serde_json::to_string(message)
            .map_err(|e| WsError::SerializationError(e.to_string()))?;
        debug!("服务端发送消息: {}", msg_json);
        self.ws_sender.send(Message::Text(msg_json)).await?;
        Ok(())
    }
}


/// 监听并接受新的 WebSocket 连接
///
/// # Arguments
/// * `addr` - 服务器绑定的地址字符串，例如 "127.0.0.1:8080"。
/// * `on_new_connection` - 一个回调闭包，当新的 WebSocket 连接建立并成功握手后被异步调用。
///   该闭包接收 `ConnectionHandler` (用于发送消息) 和 `SplitStream<WebSocketStream<TcpStream>>` (用于接收消息)。
///   闭包必须是 `FnMut` 因为它可能需要修改其捕获的状态，`Clone` 因为它会在每个新连接的任务中被克隆，
///   `Send` 和 `'static` 因为它会在 `tokio::spawn` 中被使用。
///   闭包返回一个 `Future`，该 `Future` 也必须是 `Send` 和 `'static`。
pub async fn start_server<F, Fut>(
    addr: String,
    on_new_connection: F, 
) -> Result<(), WsError>
where
    F: FnMut(ConnectionHandler, SplitStream<WebSocketStream<TcpStream>>) -> Fut + Send + Clone + 'static, 
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| WsError::IoError(e))?;
    info!("WebSocket 服务端正在监听地址: {}", addr);

    while let Ok((stream, client_addr)) = listener.accept().await {
        info!("新的 TCP 连接来自: {}", client_addr);
        let mut on_new_connection_for_task = on_new_connection.clone();
        tokio::spawn(async move {
            let callback = |req: &Request, mut response: Response|
                -> Result<Response, ErrorResponse>
            {
                info!("[握手回调] 收到来自 {} 的新 WebSocket 握手请求，路径: {}", client_addr, req.uri().path());
                response.headers_mut().append(
                    "Access-Control-Allow-Origin",
                    HeaderValue::from_static("*")
                );
                Ok(response)
            };

            match accept_hdr_async(stream, callback).await {
                Ok(ws_stream) => {
                    info!("WebSocket 连接已建立: {}", client_addr);
                    let (ws_sender, ws_receiver) = ws_stream.split();
                    let handler = ConnectionHandler { ws_sender };
                    (on_new_connection_for_task)(handler, ws_receiver).await;
                    info!("与 {} 的连接已关闭", client_addr);
                }
                Err(e) => {
                    error!("与 {} 的 WebSocket 握手失败: {}", client_addr, e);
                }
            }
        });
    }
    Ok(())
}

/// 从 WebSocket 流中接收并尝试解析一个 WsMessage。
/// 此函数处理单个消息事件，循环读取应由调用方实现。
pub async fn receive_message(
    ws_receiver: &mut SplitStream<WebSocketStream<TcpStream>>,
) -> Option<Result<WsMessage, WsError>> {
    // Loop internally only to skip over control frames (Ping, Pong, etc.) 
    // that don't yield a user-level WsMessage.
    // The main message processing loop should be in the caller.
    loop {
        match ws_receiver.next().await {
            Some(msg_result) => match msg_result {
                Ok(msg) => match msg {
                    Message::Text(text) => {
                        debug!("服务端收到文本消息: {}", text);
                        break Some(serde_json::from_str::<WsMessage>(&text)
                            .map_err(|e| WsError::DeserializationError(e.to_string())));
                    }
                    Message::Binary(bin) => {
                        debug!("服务端收到二进制消息: {:?}", bin);
                        break Some(Err(WsError::Message(
                            "收到非预期的二进制消息".to_string(),
                        )));
                    }
                    Message::Ping(ping_data) => {
                        debug!("服务端收到 Ping: {:?}. 由 tokio-tungstenite 自动处理.", ping_data);
                        // tokio-tungstenite handles pong responses automatically.
                        // Continue loop to get next actual message.
                    }
                    Message::Pong(pong_data) => {
                        debug!("服务端收到 Pong: {:?}", pong_data);
                        // Usually, Pongs are just for keep-alive; no specific action needed here for receiver.
                        // Continue loop.
                    }
                    Message::Close(close_frame) => {
                        debug!("服务端收到 Close 帧: {:?}", close_frame);
                        break None; // Connection is closing/closed.
                    }
                    Message::Frame(_) => {
                        debug!("收到一个非预期的底层 Frame 类型。正在跳过。");
                        // Continue loop for next message.
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
                break None; // Stream has ended.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::WsMessage;
    use common_models::ws_payloads::EchoPayload; 
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tokio::time::{timeout, Duration};
    // 正确导入客户端的 transport 组件, 移除 ClientConnection 因为它在测试中未被直接使用
    use crate::client::transport::{connect_client, receive_message as client_receive_message};

    async fn setup_test_server<F, Fut>(
        addr: String,
        on_conn: F,
    ) -> Result<tokio::task::JoinHandle<Result<(), WsError>>, WsError>
    where
        F: FnMut(ConnectionHandler, SplitStream<WebSocketStream<TcpStream>>) -> Fut + Send + Clone + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let server_handle = tokio::spawn(async move {
            start_server(addr, on_conn).await
        });
        tokio::time::sleep(Duration::from_millis(100)).await; 
        Ok(server_handle)
    }


    #[tokio::test]
    async fn test_server_send_receive_echo() {
        let _ = env_logger::builder().is_test(true).try_init();

        let server_bind_addr = "127.0.0.1:12345".to_string();
        let client_connect_addr = format!("ws://{}", server_bind_addr);

        let messages_received_by_server = Arc::new(Mutex::new(Vec::new()));
        let messages_received_by_server_clone = messages_received_by_server.clone();

        let server_handle = setup_test_server(server_bind_addr.clone(), move |mut handler, mut receiver| { 
            let received_arc = messages_received_by_server_clone.clone();
            async move {
                info!("[测试服务端] 新客户端已连接。");
                loop {
                    match tokio::time::timeout(Duration::from_secs(5), crate::server::transport::receive_message(&mut receiver)).await { 
                        Ok(Some(Ok(received_msg))) => {
                            info!("[测试服务端] 收到消息: {:?}", received_msg);
                            let mut received_vec = received_arc.lock().await;
                            received_vec.push(received_msg.clone());

                            if received_msg.message_type == "ClientEcho" {
                                info!("[测试服务端] 正在回显消息给客户端。");
                                if let Err(e) = handler.send_message(&received_msg).await {
                                    error!("[测试服务端] 发送回显时出错: {}", e);
                                }
                                break; 
                            }
                        }
                        Ok(Some(Err(e))) => {
                            error!("[测试服务端] 接收消息时出错: {}", e);
                            break;
                        }
                        Ok(None) => {
                            info!("[测试服务端] 连接已关闭或流在等待消息时结束。");
                            break;
                        }
                        Err(e) => {
                            error!("[测试服务端] 等待消息超时: {}", e);
                            break; 
                        }
                    }
                }
                info!("[测试服务端] 客户端处理器已完成。");
            }
        })
        .await
        .expect("测试服务端启动失败");

        
        match connect_client(client_connect_addr.clone()).await {
            Ok(mut client_conn) => {
                info!("[测试客户端] 已连接到服务端。");
                let echo_payload = EchoPayload { content: "来自客户端的问候!".to_string() };
                let message_to_send = WsMessage::new("ClientEcho".to_string(), &echo_payload)
                    .expect("创建客户端消息失败");

                info!("[测试客户端] 正在发送消息: {:?}", message_to_send);
                if let Err(e) = client_conn.send_message(&message_to_send).await {
                    error!("[测试客户端] 发送消息失败: {}", e);
                    panic!("客户端发送失败: {}", e);
                }
                info!("[测试客户端] 消息已发送。");

                
                info!("[测试客户端] 正在尝试从服务端接收回显...");
                match timeout(Duration::from_secs(5), client_receive_message(&mut client_conn.ws_receiver)).await {
                    Ok(Some(Ok(echo_response))) => {
                        info!("[测试客户端] 收到回显: {:?}", echo_response);
                        assert_eq!(echo_response.message_type, "ClientEcho");
                        let received_payload: EchoPayload = echo_response.deserialize_payload().unwrap();
                        assert_eq!(received_payload.content, "来自客户端的问候!");
                    }
                    Ok(Some(Err(e))) => panic!("[测试客户端] 接收错误: {}", e),
                    Ok(None) => panic!("[测试客户端] 未收到回显 (连接已关闭)。"),
                    Err(e) => panic!("[测试客户端] 客户端接收超时: {}", e),
                }

                let server_msgs = messages_received_by_server.lock().await;
                assert_eq!(server_msgs.len(), 1, "服务端应收到一条消息。");
                assert_eq!(server_msgs[0].message_type, "ClientEcho");
                let server_received_payload: EchoPayload = server_msgs[0].deserialize_payload().unwrap();
                assert_eq!(server_received_payload.content, "来自客户端的问候!");
            }
            Err(e) => {
                panic!("[测试客户端] 客户端连接失败 {}: {}", client_connect_addr, e);
            }
        }
        server_handle.abort();
    }
} 