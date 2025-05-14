import { ApplicationConfig, provideZoneChangeDetection } from '@angular/core';
import { provideRouter } from '@angular/router';

import { routes } from './app.routes'; // 导入应用路由定义

/**
 * `SatOnSiteMobile` (现场端移动应用) 的主应用配置对象。
 *
 * 此对象遵循 Angular 的 `ApplicationConfig` 接口，用于在应用引导时提供核心配置，
 * 包括路由、变更检测策略以及其他全局服务提供者。
 */
export const appConfig: ApplicationConfig = {
  /**
   * 应用级别的服务提供者数组。
   */
  providers: [
    // 配置 Angular 的 Zone.js 变更检测策略。
    // `eventCoalescing: true` 是一项优化，用于将多个连续的事件合并为一个变更检测周期，
    // 有助于提升性能，特别是在有大量异步事件触发时。
    provideZoneChangeDetection({ eventCoalescing: true }), 
    
    // 提供应用的主路由配置。
    // `routes` 常量是从 `./app.routes.ts` 文件导入的，定义了应用的所有顶层路由规则。
    provideRouter(routes)
  ]
};
