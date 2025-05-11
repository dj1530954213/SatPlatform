// SatOnSiteMobile/src-tauri/src/config.rs

//! `SatOnSiteMobile` (现场端) 应用的配置管理模块。
//!
//! 此模块负责定义和加载应用的配置信息，例如连接云端服务所需的 URL 等。

use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use anyhow::{Result, Context, anyhow}; // anyhow 用于更方便的错误处理和上下文添加

/// WebSocket 客户端配置结构体。
///
/// 存储与云端 WebSocket 服务建立连接所需的基本配置信息。
#[derive(Deserialize, Debug, Clone)]
pub struct WsClientConfig {
    /// 云端 WebSocket 服务的完整 URL。
    /// 例如："ws://127.0.0.1:8088/ws" 或 "wss://example.com/api/ws"
    pub cloud_ws_url: String,
}

impl WsClientConfig {
    /// 从配置文件加载 `WsClientConfig`。
    ///
    /// 当前实现是一个占位符，它会尝试从固定的相对路径 `config/onsite_client_config.json` 加载配置。
    /// 如果配置文件不存在或无法解析，将返回错误。这种情况下，依赖此配置的命令（如 `connect_to_cloud`）
    /// 将需要用户通过参数显式提供必要的配置信息（如 WebSocket URL）。
    ///
    /// # 返回
    /// * `Result<Self>`: 如果成功加载并解析配置文件，则返回 `Ok(WsClientConfig)`。
    ///   否则，返回包含错误信息的 `Err`，指明加载失败的原因。
    pub fn load() -> Result<Self> {
        // 定义配置文件的预期相对路径
        // 注意：此路径是相对于 Tauri 应用运行时的工作目录的。
        // 在开发模式 (`cargo tauri dev`)下，通常是 `src-tauri` 目录。
        // 在生产构建的应用中，路径解析可能需要更复杂的处理，例如使用 `tauri::api::path::resolve_path`。
        let config_path = PathBuf::from("config/onsite_client_config.json");
        
        // 检查配置文件是否存在
        if !config_path.exists() {
            log::warn!(
                "[SatOnSiteMobile] 配置文件 {:?} 未找到。应用将依赖命令参数传入的 URL 或后续可能实现的默认硬编码 URL。", 
                config_path
            );
            // 返回一个特定的错误，指示配置文件缺失
            // 这使得调用者（例如 `connect_to_cloud` 命令）可以据此判断是否需要强制用户提供 URL。
            return Err(anyhow!(
                "客户端配置文件 {:?} 未找到。请确保配置文件存在，或在连接时通过参数提供 WebSocket URL。", 
                config_path
            ));
        }

        // 读取配置文件内容到字符串
        let config_str = fs::read_to_string(&config_path)
            .with_context(|| format!("无法读取配置文件 {:?} 的内容。请检查文件权限和路径是否正确。", config_path))?;
        
        // 从 JSON 字符串反序列化为 WsClientConfig 结构体
        let config: WsClientConfig = serde_json::from_str(&config_str)
            .with_context(|| format!("解析配置文件 {:?} 失败。请确保文件内容是有效的 JSON 格式且符合 WsClientConfig 结构。", config_path))?;
        
        log::info!("[SatOnSiteMobile] 已成功从 {:?} 加载 WebSocket 客户端配置。云服务 URL: {}", config_path, config.cloud_ws_url);
        Ok(config)
    }
}

// 注意：如果未来有更多的配置项，可以扩展此模块。
// 例如，可以考虑从 Tauri 的 `tauri.conf.json` 或特定于平台的配置文件中读取更复杂的配置。

// 暂时为空，后续会添加配置加载和管理逻辑。 