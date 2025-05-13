// Angular 应用的主入口点 (main entry point)。
// 这个文件负责引导 (bootstrap) 应用的根模块或根组件。

import { bootstrapApplication } from '@angular/platform-browser';
import { appConfig } from './app/app.config'; // 导入应用配置
import { AppComponent } from './app/app.component'; // 导入应用的根组件

// 调用 bootstrapApplication 函数来启动 Angular 应用。
// 参数1: AppComponent - 指定应用的根组件。
// 参数2: appConfig - 传入应用级别的配置，例如路由、提供者等。
bootstrapApplication(AppComponent, appConfig)
  .catch((err) => console.error('[现场移动端引导错误]', err)); // 如果引导过程中发生错误，则捕获并在控制台打印错误信息。
