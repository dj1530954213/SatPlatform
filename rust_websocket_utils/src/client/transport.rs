// rust_websocket_utils/src/client/transport.rs

//! 客户端 WebSocket 传输层核心逻辑。
//!
//! 本模块提供了 `rust_websocket_utils` 库中用于客户端 WebSocket 通信的主要功能。
//! 它包括建立与服务器的连接、发送和接收结构化的 `WsMessage`，以及处理底层连接事件的抽象。
//! 其设计旨在简化客户端应用程序与 WebSocket 服务器的异步交互。

// 旧的 ClientTransport 结构体和实现已被移除或整合。

// use crate::error::WsClientError; // 不再使用，已统一到 WsError
use log::{info, error, debug}; // 引入日志宏，用于不同级别的日志输出
// use tokio::net::TcpStream; // TcpStream 被 ClientWsStream 间接使用
use tokio_tungstenite::{
    connect_async, // 异步连接函数
    // MaybeTlsStream, // WebSocketStream<MaybeTlsStream<...>> 这样使用即可
    WebSocketStream, // WebSocket 流类型
    // tungstenite::http::Response as TungsteniteResponse, // 不再直接使用此类型别名
    tungstenite::protocol::Message, // 底层 WebSocket 消息枚举 (Text, Binary, Ping, Pong, Close)
    tungstenite::Error as TungsteniteError, // 底层 tungstenite 库的错误类型
};
use url::Url; // 用于解析和处理 URL
use crate::error::WsError; // 引入本库定义的统一错误类型
use crate::message::WsMessage; // 引入本库定义的核心消息结构
use futures_util::{
    stream::{SplitSink, SplitStream}, // 用于将 WebSocket 流拆分为发送端和接收端
    SinkExt,    // 为 Sink（如 SplitSink）提供额外的方法，如 send()
    StreamExt,  // 为 Stream（如 SplitStream）提供额外的方法，如 next()
};

// pub struct ClientTransport; // 已移除
// impl ClientTransport { ... } // 已移除

/// `ClientWsStream` 类型别名，代表一个可能经过 TLS 加密的 TCP WebSocket 流。
/// 这是 `tokio-tungstenite` 库在客户端连接成功后返回的典型流类型。
pub type ClientWsStream = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// `ClientConnection` 结构体代表一个活动的客户端 WebSocket 连接。
///
/// 它封装了与服务器进行通信所需的发送端 (`SplitSink`) 和接收端 (`SplitStream`)。
/// 实例通常在成功连接到服务器后创建。
pub struct ClientConnection {
    /// 用于向 WebSocket 服务器异步发送消息的 `Sink` (发送端)。
    /// 消息类型为 `tungstenite::protocol::Message`，通常是 `Message::Text`。
    pub ws_sender: SplitSink<ClientWsStream, Message>,
    /// 用于从 WebSocket 服务器异步接收消息的 `Stream` (接收端)。
    /// (字段设为 `pub` 以便在测试和某些外部高级用法中直接访问，例如在 `receive_message` 之外进行轮询)。
    pub ws_receiver: SplitStream<ClientWsStream>,
}

impl ClientConnection {
    /// 异步向 WebSocket 服务器发送一个 `WsMessage`。
    ///
    /// 该方法首先将 `WsMessage` 序列化为 JSON 字符串，然后通过 WebSocket 连接发送出去。
    ///
    /// # Arguments
    /// * `message` - 一个对要发送的 `WsMessage` 实例的引用。
    ///
    /// # Returns
    /// * `Result<(), WsError>` - 如果消息成功序列化并发送，则返回 `Ok(())`。
    ///   如果序列化失败或发送过程中发生网络错误，则返回相应的 `WsError`。
    pub async fn send_message(&mut self, message: &WsMessage) -> Result<(), WsError> {
        // 将 WsMessage 序列化为 JSON 字符串
        let msg_json = serde_json::to_string(message)
            .map_err(|e| WsError::SerializationError(format!("消息序列化为JSON失败: {}", e)))?;
        debug!("客户端：准备发送消息: {}", msg_json); // 日志：记录将要发送的 JSON 消息内容
        // 通过 WebSocket 发送器发送文本类型的消息
        self.ws_sender.send(Message::Text(msg_json)).await?;
        info!("客户端：消息已成功发送 (类型: {}, ID: {:?})", message.message_type, message.message_id);
        Ok(())
    }
}

/// 异步连接到指定的 WebSocket 服务器。
///
/// 此函数尝试解析给定的 URL 字符串，然后使用 `tokio-tungstenite` 的 `connect_async` 方法
/// 建立与服务器的 WebSocket 连接。如果连接和握手成功，它会将返回的 `WebSocketStream`
/// 分割成发送端和接收端，并封装在 `ClientConnection` 结构体中返回。
///
/// # Arguments
/// * `url_str` - WebSocket 服务器的完整 URL 字符串 (例如 "ws://127.0.0.1:8080/ws" 或 "wss://example.com/socket")。
///
/// # Returns
/// * `Result<ClientConnection, WsError>` - 如果连接成功建立并完成握手，则返回包含发送和接收端的 `ClientConnection` 实例。
///   如果 URL 解析失败、连接失败或 WebSocket 握手过程中发生错误，则返回相应的 `WsError`。
pub async fn connect_client(url_str: String) -> Result<ClientConnection, WsError> {
    info!("客户端：开始尝试连接到 WebSocket 服务器，URL: {}", url_str);
    // 解析 URL 字符串
    let parsed_url = Url::parse(&url_str)
        .map_err(|e| WsError::InvalidUrl(format!("无效的 WebSocket URL '{}': {}", url_str, e)))?;

    // 异步连接到服务器
    match connect_async(parsed_url.as_str()).await {
        Ok((ws_stream, response)) => {
            // 连接成功
            info!("客户端：已成功连接到 {} (HTTP 状态码: {})", url_str, response.status());
            debug!("客户端：WebSocket 连接响应头: {:?}", response.headers());
            // 将 WebSocket 流分割为独立的发送端和接收端
            let (ws_sender, ws_receiver) = ws_stream.split();
            Ok(ClientConnection { ws_sender, ws_receiver })
        }
        Err(e) => {
            // 连接失败
            error!("客户端：连接到 {} 失败，错误: {}", url_str, e);
            Err(WsError::WebSocketProtocolError(e)) // 将底层的 TungsteniteError 包装在 WsError 中
        }
    }
}

/// 从给定的 WebSocket 接收流 (`SplitStream`) 中异步接收并尝试解析一个 `WsMessage`。
///
/// 此函数处理单个传入的 WebSocket 消息事件。它会跳过非业务相关的控制帧（如 Ping、Pong，
/// 这些通常由底层库自动处理）。如果收到文本消息，它会尝试将其反序列化为 `WsMessage` 结构体。
/// 如果收到二进制消息，则视为错误。如果连接关闭，则返回 `None`。
///
/// **注意：** 此函数设计为处理单个消息的接收和解析。在一个持续的客户端会话中，
/// 调用方通常需要在一个循环中重复调用此函数来处理所有传入的消息。
///
/// # Arguments
/// * `ws_receiver` - 一个对 `SplitStream<ClientWsStream>` 的可变引用，代表 WebSocket 连接的接收端。
///
/// # Returns
/// * `Option<Result<WsMessage, WsError>>`:
///     - `Some(Ok(ws_message))`：如果成功接收并解析了一个 `WsMessage`。
///     - `Some(Err(ws_error))`：如果在接收或解析过程中发生错误（例如，反序列化失败、收到非预期类型的消息）。
///     - `None`：如果 WebSocket 连接已关闭（例如，收到 Close 帧或流已结束）。
pub async fn receive_message(
    ws_receiver: &mut SplitStream<ClientWsStream>,
) -> Option<Result<WsMessage, WsError>> {
    // 这个内部循环主要用于处理和跳过那些不直接映射到应用层 WsMessage 的底层 WebSocket 控制帧，
    // 例如 Ping/Pong（由 tokio-tungstenite 自动处理）。
    // 主要的、持续的消息接收循环应该在调用此函数的外部实现。
    loop {
        match ws_receiver.next().await { // 等待下一条消息
            Some(msg_result) => match msg_result { // 如果流中确实有下一条消息 (Result包裹)
                Ok(msg) => match msg { // 如果成功获取了底层的 tungstenite::Message
                    Message::Text(text) => {
                        debug!("客户端：收到原始文本消息，内容: '{}'", text);
                        // 尝试将文本内容反序列化为 WsMessage
                        break Some(serde_json::from_str::<WsMessage>(&text)
                            .map_err(|e| WsError::DeserializationError(format!("收到的文本消息反序列化为 WsMessage 失败: {}, 原始文本: '{}'", e, text))));
                    }
                    Message::Binary(bin) => { 
                        debug!("客户端：收到原始二进制消息，长度: {} 字节，内容 (前50字节): {:?}", bin.len(), bin.get(..50.min(bin.len())));
                        // 根据项目设计，通常不期望客户端直接处理二进制消息，将其视为错误
                        break Some(Err(WsError::Message(
                            "客户端收到了非预期的 WebSocket 二进制消息".to_string(),
                        )));
                    }
                    Message::Ping(ping_data) => { 
                        // Ping 帧通常由 tokio-tungstenite 库自动响应 Pong，应用层无需特殊处理
                        debug!("客户端：收到 Ping 控制帧，数据: {:?}. (通常由底层库自动处理)", ping_data);
                        // 继续循环以等待下一条业务消息
                    }
                    Message::Pong(pong_data) => { 
                        // Pong 帧是作为对我们发送的 Ping 的响应，对于维护连接状态和心跳检测很重要
                        debug!("客户端：收到 Pong 控制帧，数据: {:?}", pong_data);
                        // 应用层的心跳逻辑可能会关心 Pong，但此函数仅负责消息传递，所以继续循环
                    }
                    Message::Close(close_frame) => { 
                        // 收到 Close 帧，表示连接正在关闭或已被对方关闭
                        debug!("客户端：收到 Close 控制帧，详细信息: {:?}", close_frame);
                        break None; // 表示连接已结束，没有更多消息了
                    }
                    Message::Frame(_) => { 
                        // Frame 是一个更底层的原始帧类型，通常不应在应用层面直接处理
                        debug!("客户端：收到一个非预期的底层原始 Frame 类型消息，正在跳过。");
                        // 继续循环
                    }
                },
                Err(e) => match e { // 如果从流中获取消息时发生错误 (TungsteniteError)
                    TungsteniteError::ConnectionClosed | TungsteniteError::AlreadyClosed => { 
                        // 这些错误明确表示连接已经关闭
                        debug!("客户端：连接已关闭 (在 ws_receiver.next() 期间检测到 ConnectionClosed 或 AlreadyClosed 错误)。");
                        break None; // 表示连接已结束
                    }
                    _ => {
                        // 其他类型的 TungsteniteError，例如协议错误、IO错误等
                        error!("客户端：从 WebSocket 流接收消息时发生底层错误: {}", e);
                        break Some(Err(WsError::WebSocketProtocolError(e))); // 将其包装并作为错误返回
                    }
                },
            },
            None => { // 如果 ws_receiver.next() 返回 None，表示流已经完全耗尽 (通常也意味着连接已关闭)
                debug!("客户端：WebSocket 接收流已结束 (ws_receiver.next() 返回 None)。");
                break None; // 表示连接已结束
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*; // 导入当前模块 (transport) 的所有公共项
    use crate::message::WsMessage; // 导入 WsMessage 结构
    use common_models::ws_payloads::EchoPayload; // 导入用于测试的 EchoPayload (来自共享模型)
    use tokio::time::{timeout, Duration}; // 用于测试中的超时控制
    // 导入服务端组件，用于搭建本地测试服务器
    use crate::server::transport::{
        start_server as start_echo_server, // 使用我们定义的 start_server 函数作为回显服务器的启动点
        ConnectionHandler as ServerConnectionHandler, // 服务端连接处理器类型
        receive_message as server_receive_message,    // 服务端接收消息的辅助函数
    };
    use futures_util::stream::SplitStream as ServerSplitStream; // 服务端消息接收流的类型别名
    use tokio::net::TcpStream as ServerTcpStream; // 服务端 TCP 流类型
    // use tokio_tungstenite::WebSocketStream as ServerWebSocketStream; // 移除未使用的类型别名

    // 辅助函数：启动一个简单的本地回显服务器，专门用于客户端连接和消息收发测试。
    // 这个服务器会接收客户端发来的任何 WsMessage，并将其原样发送回去。
    async fn setup_test_echo_server_for_client_tests(addr: String) -> tokio::task::JoinHandle<Result<(), WsError>> {
        tokio::spawn(async move { // 在新的异步任务中启动服务器，使其不阻塞测试主流程
            // 使用从 server::transport 模块导入的 start_server 函数
            start_echo_server(addr, move |mut conn_handler: ServerConnectionHandler, mut server_receiver: ServerSplitStream<WebSocketStream<ServerTcpStream>>| async move {
                info!("[测试回显服务端-供客户端测试]：新客户端已连接。");
                loop { // 循环处理来自该客户端的消息
                    match server_receive_message(&mut server_receiver).await {
                        Some(Ok(ws_msg)) => { // 如果成功收到并解析了一个 WsMessage
                            info!("[测试回显服务端-供客户端测试]：收到消息: {:?}，准备回显。", ws_msg);
                            // 将收到的消息原样发回给客户端
                            if let Err(e) = conn_handler.send_message(&ws_msg).await {
                                error!("[测试回显服务端-供客户端测试]：回显消息时发生错误: {}", e);
                                break; // 发送错误，终止此连接的处理循环
                            }
                        }
                        Some(Err(e)) => { // 如果接收或解析消息时发生错误
                            error!("[测试回显服务端-供客户端测试]：接收客户端消息时发生错误: {}", e);
                            break; // 接收错误，终止循环
                        }
                        None => { // 如果客户端断开连接 (receive_message 返回 None)
                            info!("[测试回显服务端-供客户端测试]：客户端已断开连接。");
                            break; // 客户端断开，终止循环
                        }
                    }
                }
            }).await // 等待 start_echo_server 完成 (通常在服务器停止时)
        })
    }

    #[tokio::test]
    /// 集成测试：测试客户端连接、发送 Echo 消息并接收回显的完整流程。
    async fn test_client_connect_send_receive_echo() {
        // 初始化日志记录器，以便在测试输出中看到日志 (如果测试失败或需要调试)
        // is_test(true) 通常会配置日志级别和输出格式，使其适合测试环境。
        let _ = env_logger::builder().is_test(true).try_init(); // 忽略结果，因为其他测试可能已初始化
        
        let server_bind_addr = "127.0.0.1:12346".to_string(); // 定义测试服务器绑定的地址和端口
        let client_connect_url = format!("ws://{}", server_bind_addr); // 构造客户端连接的 URL

        // 启动测试用的回显服务器，并获取其 JoinHandle 以便后续清理
        let server_handle = setup_test_echo_server_for_client_tests(server_bind_addr.clone()).await;
        tokio::time::sleep(Duration::from_millis(200)).await; // 短暂暂停，确保服务器有足够时间启动和监听

        // 尝试连接到测试服务器
        match connect_client(client_connect_url.clone()).await {
            Ok(mut client_conn) => { // 连接成功
                info!("[测试客户端]：已成功连接到本地回显测试服务端 URL: {}", client_connect_url);
                
                // 准备要发送的 EchoPayload 和 WsMessage
                let echo_payload = EchoPayload { content: "来自客户端集成测试的问候!".to_string() };
                let message_to_send = WsMessage::new("ClientEchoTest".to_string(), &echo_payload)
                    .expect("创建客户端 WsMessage (用于 Echo 测试) 失败，这不应发生");

                info!("[测试客户端]：准备发送 Echo 消息: {:?}", message_to_send);
                // 发送消息到服务器
                if let Err(e) = client_conn.send_message(&message_to_send).await {
                    error!("[测试客户端]：发送 Echo 消息失败: {}", e);
                    panic!("[测试客户端] 发送 Echo 消息过程中发生严重错误: {}", e);
                }
                info!("[测试客户端]：Echo 消息已成功发送至回显服务端。");

                info!("[测试客户端]：正在等待并尝试从服务端接收回显响应...");
                // 尝试接收服务器的回显，设置5秒超时以防止测试永久阻塞
                match timeout(Duration::from_secs(5), self::receive_message(&mut client_conn.ws_receiver)).await {
                    Ok(Some(Ok(response_msg))) => { // 成功收到并解析了响应消息
                        info!("[测试客户端]：成功收到回显响应消息: {:?}", response_msg);
                        // 断言：响应消息的类型应与发送的类型一致 (或服务器约定的回显类型)
                        assert_eq!(response_msg.message_type, "ClientEchoTest", "回显消息的 message_type 与预期不符");
                        // 反序列化响应的 payload
                        let received_payload: EchoPayload = response_msg.deserialize_payload()
                            .expect("反序列化回显响应的 EchoPayload 失败，这不应发生");
                        // 断言：回显的 content 应与原始发送的 content 一致
                        assert_eq!(received_payload.content, echo_payload.content, "回显的 EchoPayload 内容与原始发送的不符");
                        info!("[测试客户端]：Echo 测试成功完成！收到的内容与发送的一致。");
                    }
                    Ok(Some(Err(e))) => panic!("[测试客户端]：从回显服务端接收消息时发生错误: {}", e),
                    Ok(None) => panic!("[测试客户端]：在期望收到回显消息之前，连接意外被服务端关闭。"),
                    Err(e_timeout) => panic!("[测试客户端]：等待服务端回显响应超时 (超过5秒): {}", e_timeout),
                }
            }
            Err(e) => { // 连接失败
                error!("[测试客户端]：连接到本地回显测试服务端 {} 失败，错误: {}", client_connect_url, e);
                panic!("[测试客户端] 客户端连接到测试服务器失败，错误详情: {}", e);
            }
        }
        server_handle.abort(); // 测试完成，中止服务器任务以清理资源
        info!("[测试客户端]：Echo 测试流程结束，测试服务器已请求中止。");
    }
} 