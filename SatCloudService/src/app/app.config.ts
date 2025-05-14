import { ApplicationConfig, provideZoneChangeDetection } from '@angular/core';
import { provideRouter } from '@angular/router';
import { routes } from './app.routes';
import { provideAnimations } from '@angular/platform-browser/animations';
import { provideHttpClient } from '@angular/common/http';
import { en_US, NZ_I18N } from 'ng-zorro-antd/i18n';
import { NzMessageService } from 'ng-zorro-antd/message';
import { importProvidersFrom } from '@angular/core';
import { NzIconModule } from 'ng-zorro-antd/icon';

// 导入 CommonModule 和 ReactiveFormsModule 不是在 app.config.ts 中通过 providers 添加的
// 它们通常在组件的 imports 数组中 (对于独立组件) 或在 NgModule 的 imports 中

export const appConfig: ApplicationConfig = {
  providers: [
    provideZoneChangeDetection({ eventCoalescing: true }),
    provideRouter(routes),
    provideAnimations(),
    provideHttpClient(),
    { provide: NZ_I18N, useValue: en_US },
    NzMessageService,
    importProvidersFrom(NzIconModule)
  ]
};
