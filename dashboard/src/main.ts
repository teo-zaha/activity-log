import { provideHttpClient } from '@angular/common/http'
import { provideBrowserGlobalErrorListeners } from '@angular/core'
import { MAT_DATE_FORMATS, MAT_DATE_LOCALE } from '@angular/material/core'
import { REMOVE_STYLES_ON_COMPONENT_DESTROY, bootstrapApplication } from '@angular/platform-browser'
import { AppComponent } from './app/app.component'
import { provideAppRouter } from './app/app.routes'
import { provideDeepLinks } from './app/services/deep-link-inbox.service'
import { dismissSplash, watchSplash } from './app/utils/splash'

const GB_DATE_FORMATS = {
  parse: { dateInput: 'DD/MM/YYYY' },
  display: {
    dateInput: 'DD/MM/YYYY',
    monthYearLabel: 'MMM YYYY',
    dateA11yLabel: 'DD/MM/YYYY',
    monthYearA11yLabel: 'MMMM YYYY',
  },
}

// The root component dismisses the splash once it has rendered; a start that fails must not leave
// the splash over the error.
watchSplash()

bootstrapApplication(AppComponent, {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideHttpClient(),
    provideAppRouter(),
    provideDeepLinks(),
    { provide: MAT_DATE_LOCALE, useValue: 'en-GB' },
    { provide: MAT_DATE_FORMATS, useValue: GB_DATE_FORMATS },
    // A component's styles stay after its last instance goes. Taking a style sheet out makes
    // WebKit restyle the whole page, which on a long timeline blocks closing the event details
    // (or leaving a view) for a quarter of a second; putting it back costs nothing like that.
    { provide: REMOVE_STYLES_ON_COMPONENT_DESTROY, useValue: false },
  ],
}).catch(err => {
  dismissSplash()
  console.error(err)
})
