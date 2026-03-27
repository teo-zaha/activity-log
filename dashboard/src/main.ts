import { provideHttpClient, withFetch } from '@angular/common/http'
import { MAT_DATE_FORMATS, MAT_DATE_LOCALE } from '@angular/material/core'
import { bootstrapApplication } from '@angular/platform-browser'
import { AppComponent } from './app/app.component'
import { provideActivityBackendClient } from './app/services/backend/backend-client'

const GB_DATE_FORMATS = {
  parse: { dateInput: 'DD/MM/YYYY' },
  display: {
    dateInput: 'DD/MM/YYYY',
    monthYearLabel: 'MMM YYYY',
    dateA11yLabel: 'DD/MM/YYYY',
    monthYearA11yLabel: 'MMMM YYYY',
  },
}

bootstrapApplication(AppComponent, {
  providers: [
    provideHttpClient(withFetch()),
    provideActivityBackendClient(),
    { provide: MAT_DATE_LOCALE, useValue: 'en-GB' },
    { provide: MAT_DATE_FORMATS, useValue: GB_DATE_FORMATS },
  ],
}).catch(err => console.error(err))
