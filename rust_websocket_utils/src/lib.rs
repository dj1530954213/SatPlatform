//! `rust_websocket_utils` crate 提供了 WebSocket 通信的通用工具和抽象。
//! 
//! 它封装了底层的 WebSocket 库 (如 `tokio-tungstenite`)，
//! 提供了简化的客户端和服务器端传输层，以及消息处理的常用功能。
//! 目的是在 SatPlatform 的各个 WebSocket 组件中重用这些核心通信逻辑。

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
