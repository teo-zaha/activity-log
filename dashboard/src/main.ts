import { bootstrapApplication } from '@angular/platform-browser'
import { AppComponent } from './app/app.component'
import { appConfig } from './app/app.config'
import { dismissSplash, watchSplash } from './app/utils/splash'

// The root component dismisses the splash once it has rendered; a start that fails must not leave
// the splash over the error.
watchSplash()

bootstrapApplication(AppComponent, appConfig).catch(err => {
  dismissSplash()
  console.error(err)
})
