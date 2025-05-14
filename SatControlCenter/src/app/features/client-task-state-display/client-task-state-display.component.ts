import { Component, OnInit, OnDestroy, ChangeDetectionStrategy } from '@angular/core';
import { CommonModule } from '@angular/common';
import { Observable, Subscription, of } from 'rxjs'; // 'of' for placeholder
// import { ClientTaskStateService } from '../../core/services/client-task-state.service'; // 假设的服务
// import { TaskDebugState } from '../../core/models/task-debug-state.model'; // 假设的模型路径

// 临时的 TaskDebugState 接口定义，直到可以从 common_models 生成或在 core/models 中定义
interface TaskDebugState {
  group_id?: string;
  task_id?: string;
  general_debug_notes?: string | null;
  // 在此添加 TaskDebugState 的其他预期字段
}

// 临时的 ClientTaskStateService 占位符 (如果服务尚未创建)
// class MockClientTaskStateService {
//   currentTaskState$: Observable<TaskDebugState | null> = of({
//     group_id: 'mock-group',
//     task_id: 'mock-task',
//     general_debug_notes: 'This is a mock state from MockClientTaskStateService.'
//   });
// }

@Component({
  selector: 'app-client-task-state-display',
  standalone: true,
  imports: [CommonModule],
  templateUrl: './client-task-state-display.component.html',
  styleUrls: ['./client-task-state-display.component.scss'],
  changeDetection: ChangeDetectionStrategy.OnPush
})
export class ClientTaskStateDisplayComponent implements OnInit, OnDestroy {
  taskState$: Observable<TaskDebugState | null>;
  // private stateSubscription: Subscription | undefined;

  // constructor(private clientTaskStateService: ClientTaskStateService) { // 或 MockClientTaskStateService
  //   this.taskState$ = this.clientTaskStateService.currentTaskState$;
  // }

  // 占位构造函数，如果 ClientTaskStateService 尚未注入
  constructor() {
    this.taskState$ = of({
      group_id: 'placeholder-group',
      task_id: 'placeholder-task',
      general_debug_notes: 'Loading task state... (placeholder)'
    }); // 使用 RxJS 'of' 提供一个临时的 Observable
    console.log('ClientTaskStateDisplayComponent initialized');
  }

  ngOnInit(): void {
    // 如果是手动订阅，可以在这里进行，但使用 async pipe 更推荐
    // this.stateSubscription = this.taskState$.subscribe(state => {
    //   console.log('Task State Updated in Component:', state);
    // });
  }

  ngOnDestroy(): void {
    // if (this.stateSubscription) {
    //   this.stateSubscription.unsubscribe();
    // }
  }
} 