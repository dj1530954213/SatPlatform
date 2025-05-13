// SatOnSiteMobile/src-tauri/src/config.rs

//! `SatOnSiteMobile` (现场端移动应用) 的配置管理模块。
//!
//! 此模块定义了应用运行所需的配置信息结构，并提供了加载这些配置的逻辑。
//! 例如，它可能包含连接到云端 WebSocket 服务所需的 URL、API 端点、超时设置等。

use serde::Deserialize; // 用于从配置文件（如 JSON）反序列化数据到 Rust 结构体
use std::fs; // 用于文件系统操作，如读取文件
use std::path::PathBuf; // 用于处理文件和目录路径
use anyhow::{Result, Context, anyhow}; // anyhow 用于提供更易于管理的错误处理和上下文信息

/// WebSocket 客户端连接配置。
///
/// 本结构体存储了与云端 `SatCloudService` 建立 WebSocket 连接所必需的基本参数。
#[derive(Deserialize, Debug, Clone)] // Deserialize: 支持从数据格式反序列化; Debug: 方便调试打印; Clone: 允许创建副本
pub struct WsClientConfig {
    /// 云端 WebSocket 服务的完整 URL 地址。
    ///
    /// **示例**: `"ws://127.0.0.1:8088/ws"` (用于本地开发测试)
    /// 或 `"wss://your.cloud.server.com/api/websocket_endpoint"` (用于生产环境)。
    pub cloud_ws_url: String,
    // TODO: 根据需要可以添加更多配置项，例如：
    // pub connection_timeout_seconds: Option<u64>, // 连接超时时间（秒）
    // pub max_reconnect_attempts: Option<u32>,     // 最大重连尝试次数
}

impl WsClientConfig {
    /// 从指定的配置文件加载 `WsClientConfig` 实例。
    ///
    /// 当前实现是一个演示性的占位符，它尝试从一个固定的相对路径 `config/onsite_client_config.json` 
    /// 加载 JSON 格式的配置文件。
    ///
    /// **错误处理**: 
    /// - 如果配置文件不存在，将记录警告并返回一个特定的错误，提示用户配置文件缺失。
    /// - 如果文件存在但无法读取（例如权限问题），将返回错误。
    /// - 如果文件内容不是有效的 JSON 或不符合 `WsClientConfig` 结构，将返回解析错误。
    ///
    /// **对于调用者**: 
    /// 依赖此配置的 Tauri 命令（例如 `connect_to_cloud`）在调用此函数失败时，
    /// 可能需要提示用户通过命令参数显式提供必要的配置信息（如 WebSocket URL），
    /// 或者回退到预定义的默认配置（如果适用）。
    ///
    /// # 返回值
    /// * `Result<Self, anyhow::Error>`: 
    ///     - `Ok(WsClientConfig)`: 如果成功加载并解析了配置文件。
    ///     - `Err(anyhow::Error)`: 如果在加载或解析过程中发生任何错误，将返回包含详细上下文的错误信息。
    pub fn load() -> Result<Self> {
        // 定义配置文件的预期相对路径。
        // 注意：此路径通常是相对于 Tauri 应用运行时的工作目录。
        // - 在开发模式 (`cargo tauri dev`)下，一般是 `src-tauri` 目录。
        // - 在生产构建的应用中，路径解析可能需要更稳健的处理方式，
        //   例如使用 `tauri::api::path::resolve_path` 结合应用资源目录。
        let config_file_name = "onsite_client_config.json";
        let config_dir = "config";
        let config_path = PathBuf::from(config_dir).join(config_file_name);
        
        // 检查配置文件是否存在
        if !config_path.exists() {
            log::warn!(
                "[现场端配置] 配置文件 '{} ({})' 未找到。应用将依赖命令参数传入的 URL 或后续可能实现的默认硬编码 URL。", 
                config_file_name, config_path.display()
            );
            // 返回一个特定的错误，明确指出配置文件缺失，以便调用方可以据此做出决策。
            return Err(anyhow!(
                "客户端配置文件 '{} ({})' 未找到。请确保配置文件存在于 '{}' 目录下，或在连接时通过参数提供 WebSocket URL。", 
                config_file_name, config_path.display(), config_dir
            ));
        }

        // 读取配置文件内容到字符串
        let config_str = fs::read_to_string(&config_path)
            .with_context(|| format!(
                "无法读取配置文件 '{} ({})' 的内容。请检查文件权限和路径是否正确。", 
                config_file_name, config_path.display()
            ))?;
        
        // 从 JSON 字符串反序列化为 WsClientConfig 结构体
        let config: WsClientConfig = serde_json::from_str(&config_str)
            .with_context(|| format!(
                "解析配置文件 '{} ({})' 失败。请确保文件内容是有效的 JSON 格式且符合 WsClientConfig ({}) 结构。", 
                config_file_name, config_path.display(), std::any::type_name::<WsClientConfig>()
            ))?;
        
        log::info!(
            "[现场端配置] 已成功从 '{} ({})' 加载 WebSocket 客户端配置。云服务 URL: {}", 
            config_file_name, config_path.display(), config.cloud_ws_url
        );
        Ok(config)
    }
}

// 注意：如果未来需要管理更多的应用配置项（例如UI偏好、设备参数等），可以继续扩展此模块。
// 例如，可以考虑实现一个更通用的 `AppConfig` 结构体，其中 `WsClientConfig` 作为其一个字段。
// 也可以研究从 Tauri 的 `tauri.conf.json` 的 `plugins` 或自定义部分读取配置，
// 或者使用特定于平台的标准配置文件位置。

// 此模块目前的实现主要集中在 WsClientConfig。
// 之前的占位注释 "暂时为空，后续会添加配置加载和管理逻辑。" 已通过 WsClientConfig::load() 实现初步功能。 