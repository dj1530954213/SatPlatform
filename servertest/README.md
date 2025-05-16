# SatCloudService独立服务端

这是从SatCloudService Tauri应用中提取出来的独立后端服务，用于在无图形界面的服务器环境中运行。

## 项目结构

```
servertest/
├── src/
│   ├── api/       - 外部HTTP API接口
│   ├── config/    - 配置管理
│   ├── db/        - 数据库交互逻辑
│   ├── error/     - 错误类型定义
│   ├── mq/        - 消息队列相关功能
│   ├── state/     - 应用级别共享状态
│   ├── ws_server/ - WebSocket服务
│   ├── lib.rs     - 库入口
│   └── main.rs    - 主程序入口
└── Cargo.toml     - 项目依赖定义
```

## 后续工作

当前项目是框架结构，需要进一步迁移具体实现代码。主要包括：

1. 从SatCloudService的Tauri项目中复制核心实现代码到对应模块
2. 迁移ws_server目录下的文件：
   - connection_manager.rs
   - task_state_manager.rs
   - heartbeat_monitor.rs
   - client_session.rs
   - message_router.rs
   - service.rs

3. 确保common_models和rust_websocket_utils这两个共享库也被正确引用

## 使用方法

1. 确保common_models和rust_websocket_utils库在正确位置
2. 完成代码迁移后，可以通过以下命令构建运行：

```bash
cd servertest
cargo build
cargo run
```

默认配置将WebSocket服务绑定到127.0.0.1:8088。可以通过创建`app_settings.json`配置文件修改设置。 