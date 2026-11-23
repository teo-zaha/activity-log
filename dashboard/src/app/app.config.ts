import { provideHttpClient } from '@angular/common/http'
import { type ApplicationConfig, provideBrowserGlobalErrorListeners } from '@angular/core'
import { REMOVE_STYLES_ON_COMPONENT_DESTROY } from '@angular/platform-browser'
import { provideAppRouter } from './app.routes'
import { provideDeepLinks } from './services/deep-link-inbox.service'
import { provideLegacyImportNotice } from './services/legacy-import-notice.service'
import { provideDashboardDates } from './utils/date-adapter'

/** What the dashboard runs with; `main.ts` starts it with this. */
export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideHttpClient(),
    provideAppRouter(),
    provideDeepLinks(),
    provideLegacyImportNotice(),
    provideDashboardDates(),
    // A component's styles stay after its last instance goes. Taking a style sheet out makes
    // WebKit restyle the whole page, which on a long timeline blocks closing the event details
    // (or leaving a view) for a quarter of a second; putting it back costs nothing like that.
    { provide: REMOVE_STYLES_ON_COMPONENT_DESTROY, useValue: false },
  ],
}
