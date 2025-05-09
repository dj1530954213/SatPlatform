//! `rust_websocket_utils` 是一个提供 WebSocket 通信实用功能的 Rust Crate。
//! 它旨在简化 WebSocket 客户端和服务器的实现，特别关注与 `common_models`
//! 一起使用时的消息处理和序列化/反序列化。
//! 
//! 主要模块包括：
//! - `message`: 定义核心消息结构，如 `WsMessage`。
//! - `error`: 定义库中使用的错误类型，如 `WsServerError` 和 `WsClientError`。
//! - `server`: 提供 WebSocket 服务器端传输层和相关功能。
//! - `client`: 提供 WebSocket 客户端传输层和相关功能。

pub mod client;
pub mod error;
pub mod message;
pub mod server;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
