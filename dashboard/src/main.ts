import { bootstrapApplication } from '@angular/platform-browser'
import { provideAnimationsAsync } from '@angular/platform-browser/animations/async'
import { provideHttpClient } from '@angular/common/http'
import { MAT_DATE_LOCALE, MAT_DATE_FORMATS } from '@angular/material/core'
import { AppComponent } from './app/app.component'

const GB_DATE_FORMATS = {
  parse:   { dateInput: 'DD/MM/YYYY' },
  display: {
    dateInput:          'DD/MM/YYYY',
    monthYearLabel:     'MMM YYYY',
    dateA11yLabel:      'DD/MM/YYYY',
    monthYearA11yLabel: 'MMMM YYYY',
  },
}

bootstrapApplication(AppComponent, {
  providers: [
    provideAnimationsAsync(),
    provideHttpClient(),
    { provide: MAT_DATE_LOCALE,   useValue: 'en-GB' },
    { provide: MAT_DATE_FORMATS,  useValue: GB_DATE_FORMATS },
  ],
}).catch(err => console.error(err))
