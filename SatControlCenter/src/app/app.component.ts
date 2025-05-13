import { Component, OnInit, OnDestroy } from '@angular/core';
import { RouterModule } from '@angular/router';
import { WebSocketService } from './core/services/websocket.service'; // 导入 WebSocket 服务
import { Observable, Subscription } from 'rxjs'; // 导入 Observable 和 Subscription
import { CommonModule } from '@angular/common'; // 导入 CommonModule 以使用 async pipe 和 ngIf/ngFor
import { FormsModule } from '@angular/forms'; // 导入 FormsModule 以使用 ngModel
import {
    WsConnectionStatusEvent,
    WsRegistrationStatusEvent,
    WsPartnerStatusEvent,
    TaskDebugState
} from './core/models/tauri-events.model'; // 导入事件和状态模型

// WebSocket 连接状态事件的 Payload (与 Rust 端 WsConnectionStatusEvent 对应)
interface WsConnectionStatusEventPayload {
  connected: boolean;
  client_id?: string | null;
  error_message?: string | null;
}

// 客户端注册状态事件的 Payload
interface WsRegistrationStatusEventPayload {
  success: boolean;
  message?: string | null;
  group_id?: string | null; // 确认注册到的组ID
  task_id?: string | null;  // 确认注册到的任务ID
}

// 伙伴客户端状态事件的 Payload
interface WsPartnerStatusEventPayload {
  partner_role: string; // 例如 'ControlCenter' 或 'OnSiteMobile'
  is_online: boolean;
}

// 本地任务状态更新事件的 Payload (TaskDebugState 的具体结构后续定义)
interface LocalTaskStateUpdatedEventPayload {
  new_state: any; // 暂时用 any，后续应替换为 TaskDebugState 接口
}

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [
    RouterModule, 
    CommonModule, // 添加 CommonModule
    FormsModule   // 添加 FormsModule
  ],
  templateUrl: './app.component.html',
  styleUrls: ['./app.component.css']
})
export class AppComponent implements OnInit, OnDestroy {
  title = 'SatControlCenter';

  // --- WebSocket 相关属性 ---
  // 用于表单输入的属性
  wsUrl: string = 'ws://localhost:8080/ws'; // 默认的 WebSocket URL
  groupId: string = 'testgroup1';
  taskId: string = 'task101';

  // 从服务获取的状态 Observables
  connectionStatus$: Observable<WsConnectionStatusEvent | null>;
  registrationStatus$: Observable<WsRegistrationStatusEvent | null>;
  partnerStatus$: Observable<WsPartnerStatusEvent | null>;
  taskState$: Observable<TaskDebugState | null>;

  // 存储订阅，以便在组件销毁时取消订阅 (虽然 async pipe 会处理，但以防未来有手动订阅)
  private subscriptions: Subscription = new Subscription();

  // 用于UI绑定的属性
  connectionStatusMessage: string = '未连接';
  registrationStatusMessage: string = '未注册';
  partnerStatusMessage: string = '伙伴状态未知';
  taskStateString: string = '任务状态未初始化';

  // --- Echo 功能相关属性 ---
  echoInput: string = '你好，云端！来自 SatControlCenter。'; // 默认 Echo 内容
  echoResponseMessage: string = '等待 Echo 回复...';

  /**
   * 构造函数，注入 WebSocketService。
   * @param webSocketService WebSocket 服务实例。
   */
  constructor(private webSocketService: WebSocketService) {
    // 从服务中获取 Observables
    this.connectionStatus$ = this.webSocketService.connectionStatus$;
    this.registrationStatus$ = this.webSocketService.registrationStatus$;
    this.partnerStatus$ = this.webSocketService.partnerStatus$;
    this.taskState$ = this.webSocketService.taskState$;
  }

  ngOnInit(): void {
    // 可以在这里添加一些初始化逻辑，例如订阅 echo 响应
    const echoSub = this.webSocketService.echoResponse$.subscribe(payload => {
      console.log('AppComponent: Received echo response:', payload.content);
      // 可以在这里更新 UI 或显示通知
    });
    this.subscriptions.add(echoSub);
  }

  /**
   * 处理连接按钮点击事件，调用 WebSocketService 的 connectToCloud 方法。
   */
  connect(): void {
    console.log(`AppComponent: Attempting to connect to ${this.wsUrl}`);
    this.webSocketService.connectToCloud(this.wsUrl)
      .then(() => console.log('AppComponent: Connect command sent successfully.'))
      .catch(err => console.error('AppComponent: Error sending connect command:', err));
  }

  /**
   * 处理注册按钮点击事件，调用 WebSocketService 的 registerClientWithTask 方法。
   */
  register(): void {
    if (!this.groupId || !this.taskId) {
      console.error('AppComponent: Group ID and Task ID are required for registration.');
      alert('请输入组ID和任务ID！'); // 简单的用户提示
      return;
    }
    console.log(`AppComponent: Attempting to register with Group ID: ${this.groupId}, Task ID: ${this.taskId}`);
    this.webSocketService.registerClientWithTask(this.groupId, this.taskId)
      .then(() => console.log('AppComponent: Register command sent successfully.'))
      .catch(err => console.error('AppComponent: Error sending register command:', err));
  }

  /**
   * (仅用于测试) 发送 Echo 消息。
   */
  sendEchoTest(): void {
    const content = `Echo from Angular at ${new Date().toLocaleTimeString()}`;
    this.webSocketService.sendEcho(content)
      .then(() => console.log('AppComponent: Echo command sent.'))
      .catch(err => console.error('AppComponent: Error sending echo command:', err));
  }

  /**
   * 组件销毁时取消所有订阅。
   */
  ngOnDestroy(): void {
    this.subscriptions.unsubscribe();
    // WebSocketService 内部的 ngOnDestroy 会处理 unlisten
  }
}
