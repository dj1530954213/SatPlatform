import { Routes } from '@angular/router';

/**
 * `SatOnSiteMobile` (现场端移动应用) 的主路由配置数组。
 *
 * 此数组 (`routes`) 定义了应用的所有顶层路由规则。
 * 每个路由对象可以指定路径 (`path`)、要加载的组件 (`component`)、子路由 (`children`)、
 * 路由守卫 (`canActivate`, `canDeactivate`)、数据解析 (`resolve`) 等。
 *
 * **当前状态**: 路由配置为空。这意味着应用当前只有一个根视图，由 `AppComponent` 提供。
 * 后续开发中，随着功能的增加（例如，引入不同的任务视图、设置页面、详情页面等），
 * 将在此数组中添加相应的路由定义。
 *
 * **示例 (未来可能添加的路由):**
 * ```typescript
 * // import { TaskListComponent } from './tasks/task-list/task-list.component';
 * // import { TaskDetailComponent } from './tasks/task-detail/task-detail.component';
 * // import { SettingsComponent } from './settings/settings.component';
 *
 * // export const routes: Routes = [
 * //   { path: '', redirectTo: '/tasks', pathMatch: 'full' }, // 默认重定向到任务列表
 * //   { path: 'tasks', component: TaskListComponent },
 * //   { path: 'task/:id', component: TaskDetailComponent },
 * //   { path: 'settings', component: SettingsComponent },
 * //   // { path: '**', component: PageNotFoundComponent } // 捕获所有未匹配路由
 * // ];
 * ```
 */
export const routes: Routes = [];
