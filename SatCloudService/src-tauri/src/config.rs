// SatCloudService/src-tauri/src/config.rs

use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use tauri::Manager;

/// WebSocket 服务的默认主机地址
pub const DEFAULT_WS_HOST: &str = "0.0.0.0";
/// WebSocket 服务的默认端口号 (注意：端口从常用的 8080 修改为 8088 以避免冲突)
pub const DEFAULT_WS_PORT: u16 = 8088;

/// 用于从 tauri.conf.json 解析插件配置的辅助结构体。
/// 注意：当前版本的 SatCloudService 并未直接通过 Tauri 插件配置机制来加载 WebSocket 服务器的具体设置，
/// 而是采用了独立的 `app_settings.json` 文件。此结构体可能为未来扩展或不同配置方式预留。
#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)] // 允许未使用的代码，因为当前配置加载逻辑不直接使用此结构体
struct WsPluginSettings {
    /// 主机地址
    host: Option<String>,
    /// 端口号
    port: Option<u16>,
}

/// 旧版 WebSocket 服务配置结构体。
/// 此结构体在项目早期版本 (P1.1.1 阶段) 中使用，
/// 现在其主要功能已由 `WebSocketConfig` 和 `AppConfig` 更全面地替代。
/// 保留它可能是为了兼容性或逐步迁移的考虑。
#[derive(Debug, Clone)]
pub struct WsConfig {
    /// WebSocket 服务监听的主机地址。
    pub host: String,
    /// WebSocket 服务监听的端口号。
    pub port: u16,
}

// 为旧版 WsConfig 实现 Default trait，提供默认配置值。
impl Default for WsConfig {
    fn default() -> Self {
        WsConfig {
            host: DEFAULT_WS_HOST.to_string(), // 默认主机地址
            port: DEFAULT_WS_PORT,            // 默认端口号
        }
    }
}

// 旧版 WsConfig 的实现块。
impl WsConfig {
    /// 加载 WebSocket 配置（旧版方法）。
    ///
    /// 重要提示：此方法在 P1.1.1 开发阶段其功能被显著简化。
    /// 当前它的主要作用是记录一条日志信息并直接返回硬编码的默认配置。
    /// 它并**不**从 `tauri.conf.json` 文件或环境变量中动态解析详细的配置信息；
    /// 这部分更完善的配置加载逻辑已转移到新的 `AppConfig` 结构以及 `load_or_create_config` 函数中进行处理。
    ///
    /// 保留此方法可能是为了兼容旧有代码、支持逐步迁移过程，或作为未来某种特定配置场景的备用。
    ///
    /// # 参数
    /// * `_app_handle`: `tauri::AppHandle` 类型的应用句柄。此参数当前在函数体内未被使用，
    /// 但保留在函数签名中，可能是为了保持API的一致性或为将来的扩展性做准备。
    pub fn load(_app_handle: &tauri::AppHandle) -> Self {
        let default_config = WsConfig::default();
        // 日志记录：明确指出当前使用的是默认配置，并提示新的配置机制。
        log::info!(
            "[配置模块::WsConfig] 旧版配置加载：当前使用默认 WebSocket 配置: 主机={}, 端口={}. 更完善的配置加载已由 AppConfig 处理。",
            default_config.host,
            default_config.port
        );
        default_config
    }
}

// 全局静态应用配置实例，使用 `OnceLock` 确保其线程安全且仅被初始化一次。
// 这是存储整个应用配置（通过 `AppConfig` 结构定义）的地方。
static APP_CONFIG: OnceLock<AppConfig> = OnceLock::new();

/// WebSocket 服务端详细配置结构体。
/// 定义了 WebSocket 服务运行所需的各项参数。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WebSocketConfig {
    /// WebSocket 服务绑定的主机地址。
    pub host: String,
    /// WebSocket 服务监听的端口号。
    pub port: u16,
    /// 心跳检查的间隔时间（单位：秒）。
    /// `HeartbeatMonitor` 会以此频率检查客户端的活跃状态。
    pub heartbeat_check_interval_seconds: u64,
    /// 客户端超时时间（单位：秒）。
    /// 如果客户端在此时间内无任何活动（未发送消息，包括 Ping），则认为其超时并断开连接。
    pub client_timeout_seconds: u64,
}

// 为 WebSocketConfig 实现 Default trait，提供一组合理的默认值。
// 当配置文件缺失或无法解析时，将使用这些默认设置。
impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),      // 默认监听本地回环地址
            port: 8088,                         // 默认监听 8088 端口
            heartbeat_check_interval_seconds: 15, // 默认每 15 秒检查一次心跳
            client_timeout_seconds: 60,         // 默认客户端 60 秒无响应则超时
        }
    }
}

/// 应用的主配置结构体，整合了所有模块的配置信息。
/// 目前主要包含 WebSocket 服务的配置，未来可扩展以包含其他模块（如数据库、消息队列等）的配置。
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AppConfig {
    /// WebSocket 服务的相关配置。
    pub websocket: WebSocketConfig,
    // 在此可以添加其他配置项，例如：
    // pub database: DatabaseConfig,
    // pub message_queue: MessageQueueConfig,
}

/// 加载或创建应用配置文件 (`app_settings.json`)。
///
/// 此函数负责从应用特定的配置目录中读取名为 `app_settings.json` 的配置文件。
/// 处理流程如下：
/// 1. 获取应用配置目录的路径 (例如 `C:\Users\<User>\AppData\Roaming\<AppName>`)。
/// 2. 在该目录下创建或确认一个特定于本应用 (`SatCloudService`) 的子目录。
/// 3. 构造配置文件的完整路径 (`...\SatCloudService\app_settings.json`)。
/// 4. 尝试从该路径读取文件内容：
///    a. 如果文件存在且内容可被成功反序列化为 `AppConfig` 结构体，则返回解析得到的配置。
///    b. 如果文件内容反序列化失败（例如 JSON 格式错误），则记录警告，使用默认配置，并尝试用默认配置覆盖原文件。
///    c. 如果文件不存在或读取失败（例如权限问题），则记录提示，使用默认配置，并尝试创建新的配置文件写入默认配置。
///
/// # 参数
/// * `app_handle`: `tauri::AppHandle` 类型的应用句柄，用于获取应用相关的路径信息。
///
/// # 返回
/// 返回加载或创建的 `AppConfig` 实例。
fn load_or_create_config(app_handle: &tauri::AppHandle) -> AppConfig {
    // 1. 获取应用配置目录的路径
    let config_dir = match app_handle.path().app_config_dir() {
        Ok(path) => path, // 成功获取路径
        Err(e) => {      // 获取路径失败
            // 使用 eprintln! 直接输出到 stderr，适用于启动早期可能日志系统尚未完全初始化的情况
            eprintln!("[配置模块] 严重错误：无法获取应用配置目录路径: {}. 将回退到默认配置。", e);
            return AppConfig::default(); // 返回默认配置，程序将以默认设置运行
        }
    };

    // 2. 创建或确认特定于 SatCloudService 的配置子目录
    let app_specific_config_dir = config_dir.join("SatCloudService");
    if !app_specific_config_dir.exists() { // 如果子目录不存在
        if let Err(e) = fs::create_dir_all(&app_specific_config_dir) { // 尝试递归创建它
            eprintln!(
                "[配置模块] 错误：无法创建应用专属配置目录 {:?} : {}. 将使用默认配置。",
                app_specific_config_dir,
                e
            );
            return AppConfig::default(); // 创建失败则返回默认配置
        }
    }

    // 3. 定义配置文件的完整路径
    let config_file_path = app_specific_config_dir.join("app_settings.json");

    // 4. 尝试读取和解析配置文件
    match fs::read_to_string(&config_file_path) {
        Ok(content) => { // 文件读取成功
            // 尝试将文件内容 (JSON字符串) 反序列化为 AppConfig 结构体
            match serde_json::from_str::<AppConfig>(&content) {
                Ok(config) => { // 反序列化成功
                    info!("[配置模块] 已成功从配置文件 {:?} 加载应用配置。", config_file_path);
                    config // 返回从文件加载到的配置
                }
                Err(e) => { // 反序列化失败 (例如 JSON 格式损坏)
                    warn!(
                        "[配置模块] 警告：从 {:?} 反序列化配置失败: {}. 文件可能已损坏。将使用默认配置并尝试覆盖。",
                        config_file_path, e
                    );
                    let default_config = AppConfig::default(); // 使用默认配置
                    save_config(&default_config, &config_file_path); // 尝试保存默认配置以修复/创建文件
                    default_config // 返回默认配置
                }
            }
        }
        Err(e) => { // 文件读取失败 (例如文件不存在或无读取权限)
            info!(
                "[配置模块] 未在 {:?} 找到配置文件或读取时发生错误 (错误: {}). 将使用默认配置并尝试创建新文件。",
                config_file_path, e // 加入具体的错误信息 e
            );
            let default_config = AppConfig::default(); // 使用默认配置
            save_config(&default_config, &config_file_path); // 尝试创建并保存默认配置文件
            default_config // 返回默认配置
        }
    }
}

/// 将应用配置序列化为 JSON 格式并保存到指定的文件路径。
/// 此函数主要用于在配置文件不存在或损坏时，将默认配置写入磁盘。
///
/// # 参数
/// * `config`: 对要保存的 `AppConfig` 实例的引用。
/// * `path`: `PathBuf` 类型，指定了配置文件的完整保存路径。
fn save_config(config: &AppConfig, path: &PathBuf) {
    // 尝试将 AppConfig 实例美化（格式化以便阅读）并序列化为 JSON 字符串
    match serde_json::to_string_pretty(config) {
        Ok(content) => { // 序列化成功
            // 尝试将序列化后的 JSON 字符串内容写入到指定的文件路径
            if let Err(e) = fs::write(path, content) {
                warn!("[配置模块] 错误：将配置写入文件 {:?} 时失败: {}", path, e);
            } else {
                info!("[配置模块] 已成功将当前配置（可能是默认配置）保存到 {:?}.", path);
            }
        }
        Err(e) => { // 序列化为 JSON 字符串失败
            warn!("[配置模块] 错误：序列化配置信息以便保存时失败: {}", e);
        }
    }
}

/// 初始化全局应用配置。
///
/// 此函数应在应用启动过程的早期被调用一次。它的主要职责是：
/// 1. 调用 `load_or_create_config` 函数来加载现有的配置文件（如果存在且有效），
///    或者创建一份基于默认值的配置（如果配置文件不存在或损坏）。
/// 2. 将加载或创建得到的 `AppConfig` 实例存储到全局静态变量 `APP_CONFIG` (类型为 `OnceLock<AppConfig>`) 中。
///    `OnceLock` 确保配置只被设置一次，防止后续意外修改。
///
/// 如果 `APP_CONFIG` 已经被初始化过（例如，此函数被错误地调用了多次），
/// `set` 方法会返回一个错误，此时会记录一条警告日志，但不会覆盖已有的配置。
///
/// # 参数
/// * `app_handle`: `tauri::AppHandle` 类型的应用句柄，传递给 `load_or_create_config` 以便访问应用路径。
pub fn init_config(app_handle: &tauri::AppHandle) {
    let loaded_config = load_or_create_config(app_handle);
    if APP_CONFIG.set(loaded_config).is_err() {
        // 使用 eprintln! 因为这表示一个潜在的严重初始化问题
        eprintln!("[配置模块] 严重警告：全局应用配置 APP_CONFIG 已经被初始化过了。");
        // 也使用 warn! 记录到标准日志流
        warn!("[配置模块] 全局应用配置 APP_CONFIG 已被初始化，本次 init_config 调用未覆盖已有配置。请检查初始化流程。");
    }
    info!("[配置模块] 应用配置已成功初始化完毕。");
}

/// 获取对已加载的全局应用配置的静态只读引用。
///
/// **重要：** 此函数依赖于 `init_config` 函数已被成功调用并初始化了全局配置。
/// 如果在 `init_config` 调用之前或失败之后调用 `get_config`，
/// `APP_CONFIG.get()` 方法会返回 `None`，而 `.expect()` 会导致程序 panic。
/// 因此，调用者必须确保 `init_config` 已正确执行。
///
/// # Panics
/// 如果 `APP_CONFIG` 尚未通过 `init_config` 初始化，则此函数会 panic。
///
/// # 返回
/// 返回一个对全局 `AppConfig` 实例的静态生命周期引用 (`&'static AppConfig`)。
pub fn get_config() -> &'static AppConfig {
    APP_CONFIG.get().expect("[配置模块] 致命错误：APP_CONFIG 尚未初始化。请确保在使用 get_config 前已成功调用 init_config。")
} 