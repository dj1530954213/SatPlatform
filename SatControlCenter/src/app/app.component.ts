import { Component, OnInit, OnDestroy, ChangeDetectorRef } from '@angular/core';
import { FormsModule } from '@angular/forms'; // 保留 FormsModule，中心端也有
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { TauriListenerService, PartnerInfo } from './core/services/tauri-listener.service'; // 确保路径正确
import { CommonModule } from '@angular/common'; // 引入 CommonModule
import { DebugSenderComponent } from './features/debug-tools/debug-sender/debug-sender.component'; // <-- 修正导入路径

// WebSocket 连接状态事件的 Payload (与 Rust 端 WsConnectionStatusEventPayload 对应)
interface WsConnectionStatusEventPayload {
  /**
   * @description 是否已连接
   */
  connected: boolean;
  /**
   * @description 客户端ID，连接成功时由云端分配
   */
  client_id?: string | null;
  /**
   * @description 错误信息，连接失败或断开时可能包含
   */
  error_message?: string | null;
}

// 客户端注册状态事件的 Payload
interface WsRegistrationStatusEventPayload {
  /**
   * @description 注册是否成功
   */
  success: boolean;
  /**
   * @description 附加消息，成功或失败时可能包含更详细的说明
   */
  message?: string | null;
  /**
   * @description 客户端成功注册后分配到的组ID
   */
  group_id?: string | null;
  /**
   * @description 客户端成功注册后分配到的任务ID
   */
  task_id?: string | null;
  /**
   * @description （可选）云端分配给客户端的唯一标识，注册成功时返回。
   * @remark Rust 端事件 WsRegistrationStatusEvent 包含 assigned_client_id
   */
  assigned_client_id?: string | null;
}

// Echo 响应事件的 Payload (与 Rust 端 EchoResponseEventPayload 对应)
interface EchoResponseEventPayload {
  /**
   * @description Echo响应的具体内容
   */
  content: string;
}

// 本地任务状态更新事件的 Payload (Rust 端发送 TaskDebugState)
interface LocalTaskStateUpdatedEventPayload {
  /**
   * @description 新的任务状态对象。其具体结构应与 common_models 中定义的 TaskDebugState 一致。
   *              在TypeScript中通常表示为 any 或更具体的接口（如果已定义）。
   */
  new_state: any;
}

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [FormsModule, CommonModule, DebugSenderComponent], // <-- 添加到 imports
  templateUrl: './app.component.html',
  styleUrls: ['./app.component.css']
})
export class AppComponent implements OnInit, OnDestroy {
  // private unlistenConnectionStatus?: UnlistenFn; // 由 TauriListenerService 处理
  // private unlistenRegistrationStatus?: UnlistenFn; // 由 TauriListenerService 处理
  private unlistenPartnerStatus?: UnlistenFn; // 由 TauriListenerService 处理, 但我们直接用其 Observable
  // private unlistenTaskStateUpdate?: UnlistenFn; // 由 TauriListenerService 处理
  // private unlistenEcho?: UnlistenFn; // 由 TauriListenerService 处理

  /**
   * @description WebSocket 连接状态的文字描述，用于UI显示
   */
  connectionStatusMessage: string = '等待连接...';
  /**
   * @description 客户端注册状态的文字描述，用于UI显示
   */
  registrationStatusMessage: string = '尚未注册';
  // partnerStatusMessage: string = '伙伴状态未知'; // 将由下面的 partnerInfo 对象驱动
  /**
   * @description 任务状态的字符串表示，用于UI显示（通常是JSON格式化后的 TaskDebugState）
   */
  taskStateString: string = '任务状态未初始化';

  /**
   * @description Echo 功能的输入内容，双向绑定到UI输入框
   */
  echoInput: string = '你好，云端！来自 SatControlCenter。';
  /**
   * @description Echo 功能收到的回复消息，用于UI显示
   */
  echoResponseMessage: string = '等待 Echo 回复...';

  /**
   * @description 目标任务组ID，双向绑定到UI输入框
   */
  groupId: string = 'test_group_01';
  /**
   * @description 目标任务ID，双向绑定到UI输入框
   */
  taskId: string = 'test_task_001';

  /**
   * @description 用于存储从服务获取的伙伴信息
   */
  partnerInfo: PartnerInfo | null = null;
  /**
   * @description 用于保存对伙伴状态 Observable 的订阅，以便在组件销毁时取消订阅
   */
  private partnerStatusSubscription: any; // 类型应为 Subscription，但原始文件为 any，保持不变

  constructor(private cdr: ChangeDetectorRef, private tauriListenerService: TauriListenerService) {
    console.log('[中心端] AppComponent 组件构造函数初始化');
  }

  async ngOnInit(): Promise<void> {
    console.log('[中心端] AppComponent 组件 ngOnInit 生命周期钩子执行');

    // 初始化 Tauri 事件监听器 (TauriListenerService 会处理所有事件的监听设置)
    await this.tauriListenerService.initializeListeners();
    console.log('[中心端 AppComponent] Tauri 事件监听器已通过服务初始化');

    // 订阅连接状态从服务
    this.tauriListenerService.connectionStatus$.subscribe(payload => {
      if (payload) {
        this.connectionStatusMessage = payload.connected
          ? `已连接 (客户端ID: ${payload.client_id || '未知'})`
          : `已断开 (原因: ${payload.error_message || '未知原因'})`;
        this.cdr.detectChanges();
      }
    });

    // 订阅注册状态从服务
    this.tauriListenerService.registrationStatus$.subscribe(payload => {
      if (payload) {
        if (payload.success) {
          this.registrationStatusMessage = `注册成功 (分配客户端ID: ${payload.assigned_client_id || '未分配'}, 组ID: ${payload.group_id || '无'}, 任务ID: ${payload.task_id || '无'})`;
        } else {
          this.registrationStatusMessage = `注册失败: ${payload.message || '未知错误'}`;
        }
        this.cdr.detectChanges();
      }
    });

    // 新增：订阅伙伴状态从服务
    this.partnerStatusSubscription = this.tauriListenerService.partnerStatus$.subscribe(info => {
      console.log('[中心端 AppComponent] 从服务接收到伙伴信息更新:', info);
      this.partnerInfo = info;
      this.cdr.detectChanges(); // 手动触发变更检测
    });

    // 订阅任务状态从服务
    this.tauriListenerService.taskState$.subscribe(payload => {
      if (payload && payload.new_state) {
        this.taskStateString = JSON.stringify(payload.new_state, null, 2);
      } else {
        this.taskStateString = '收到的任务状态为空或无效';
      }
      this.cdr.detectChanges();
    });
    
    // 订阅Echo响应从服务
    this.tauriListenerService.echoResponse$.subscribe(payload => {
      if(payload && payload.content) {
        this.echoResponseMessage = `Echo 回复: ${payload.content}`;
      } else {
        this.echoResponseMessage = '收到空的 Echo 回复';
      }
      this.cdr.detectChanges();
    });

    // 移除旧的直接监听逻辑
    // if (!this.unlistenConnectionStatus) { ... }
    // if (!this.unlistenRegistrationStatus) { ... }
    // if (!this.unlistenPartnerStatus) { ... } // 这部分已被上面的订阅替代
    // if (!this.unlistenTaskStateUpdate) { ... }
    // if (!this.unlistenEcho) { ... }
  }

  ngOnDestroy(): void {
    console.log('[中心端] AppComponent 组件 ngOnDestroy 生命周期钩子执行');
    
    // 取消伙伴状态的订阅
    if (this.partnerStatusSubscription) {
      this.partnerStatusSubscription.unsubscribe();
    }

    // 组件销毁时清理 TauriListenerService 中的所有监听器
    this.tauriListenerService.destroyListeners();
    console.log('[中心端 AppComponent] 已通过服务销毁 Tauri 事件监听器');

    // 移除旧的 unlisten 调用
    // if (this.unlistenConnectionStatus) { this.unlistenConnectionStatus(); }
    // if (this.unlistenRegistrationStatus) { this.unlistenRegistrationStatus(); }
    // if (this.unlistenPartnerStatus) { this.unlistenPartnerStatus(); }
    // if (this.unlistenTaskStateUpdate) { this.unlistenTaskStateUpdate(); }
    // if (this.unlistenEcho) { this.unlistenEcho(); }
  }

  /**
   * @description 调用 Tauri 后端命令，请求连接到 WebSocket 云服务。
   *              连接地址硬编码为 'ws://127.0.0.1:8088'。
   */
  async connectToCloud() {
    console.log('[中心端 AppComponent] 用户请求连接到云服务。');
    try {
      // 调用 Tauri 后端命令 connect_to_cloud，不传递 URL 则使用默认配置
      console.log('[中心端 AppComponent] 调用 Tauri 命令: connect_to_cloud');
      const connectionUrl = 'ws://66.103.223.51:8088'; 
      console.log(`[中心端 AppComponent] 尝试连接到 WebSocket 服务地址: ${connectionUrl}`);
      await invoke('connect_to_cloud', { url: connectionUrl })
        .then(() => {
          console.log('[中心端 AppComponent] connect_to_cloud 命令已成功发送至 Rust 后端');
        })
        .catch(error => {
          console.error('[中心端 AppComponent] connect_to_cloud 命令调用失败:', error);
          this.connectionStatusMessage = `连接命令调用失败: ${error}`;
          this.cdr.detectChanges();
        });
      console.log('[中心端 AppComponent] connect_to_cloud 命令调用成功。');
      // 实际的连接状态将通过 ws_connection_status 事件更新
      // this.connectionStatusMessage = '正在尝试连接...'; // 可以选择性地立即更新UI
    } catch (error) {
      console.error('[中心端 AppComponent] 调用 connect_to_cloud 命令失败:', error);
      this.connectionStatusMessage = `连接命令调用失败: ${error}`;
    }
  }

  /**
   * @description 调用 Tauri 后端命令，触发 WebSocket 客户端的重连逻辑（如果后端已实现）。
   *              此函数目前主要用于演示，实际重连逻辑通常由客户端服务内部自动处理。
   */
  async triggerReconnect() {
    console.warn(
      '[中心端 AppComponent] triggerReconnect 被调用，但此功能通常由后端服务自动处理。'
    );
    // 实际的重连逻辑应该在 Rust 端的 WebSocketClientService 中实现，
    console.log('[中心端 AppComponent] 调用 Tauri 命令: trigger_reconnect (如果后端已实现此命令)');
    await invoke('trigger_reconnect')
      .then(() => console.log('[中心端 AppComponent] trigger_reconnect 命令已成功发送至 Rust 后端'))
      .catch(error => console.error('[中心端 AppComponent] trigger_reconnect 命令调用失败:', error));
  }

  /**
   * @description 调用 Tauri 后端命令，请求将此客户端注册到指定的任务组和任务。
   *              需要用户在UI中输入 groupId 和 taskId。
   */
  async registerClient() {
    console.log('[中心端 AppComponent] 用户请求注册客户端到任务组。');
    if (!this.groupId || !this.taskId) {
      this.registrationStatusMessage = '错误：组ID和任务ID不能为空。';
      console.error('[中心端 AppComponent] 注册失败: 组ID或任务ID为空。');
      return;
    }
    try {
      console.log(
        `[中心端 AppComponent] 正在调用 register_client_with_task 命令, 组ID: ${this.groupId}, 任务ID: ${this.taskId}`
      );
      await invoke('register_client_with_task', {
        group_id: this.groupId,
        task_id: this.taskId
      });
      // this.registrationStatusMessage = '注册请求已发送，等待云端响应...'; // 可选的即时UI反馈
      console.log(
        '[中心端 AppComponent] register_client_with_task 命令调用成功。实际注册状态将通过事件更新。'
      );
    } catch (error) {
      console.error(
        `[中心端 AppComponent] 调用 register_client_with_task 命令 (组ID: ${this.groupId}, 任务ID: ${this.taskId}) 失败:`,
        error
      );
      this.registrationStatusMessage = `注册命令调用失败: ${error}`;
    }
  }

  /**
   * @description 调用 Tauri 后端命令，发送 Echo 消息到云服务。
   *              Echo 内容从 UI 输入框获取。
   */
  async sendEcho() {
    console.log('[中心端 AppComponent] 用户请求发送 Echo 消息。');
    if (!this.echoInput) {
      this.echoResponseMessage = 'Echo 内容不能为空。';
      return;
    }
    try {
      console.log(
        `[中心端 AppComponent] 正在调用 send_ws_echo 命令, 内容: "${this.echoInput}"`
      );
      await invoke('send_ws_echo', { content: this.echoInput });
      this.echoResponseMessage = 'Echo 消息已发送，等待回复...';
      console.log('[中心端 AppComponent] send_ws_echo 命令调用成功。');
    } catch (error) {
      console.error(
        `[中心端 AppComponent] 调用 send_ws_echo 命令 (内容: "${this.echoInput}") 失败:`,
        error
      );
      this.echoResponseMessage = `发送 Echo 失败: ${error}`;
    }
  }
}
