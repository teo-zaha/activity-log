import { Component, OnInit, effect, inject } from '@angular/core'
import { MatProgressBarModule } from '@angular/material/progress-bar'
import {
  MatSnackBar,
  MatSnackBarModule,
  type MatSnackBarRef,
  type TextOnlySnackBar,
} from '@angular/material/snack-bar'
import { ActivityService } from './services/activity.service'
import { ThemeService } from './services/theme.service'
import { HeaderComponent } from './components/header/header.component'
import { SidebarComponent } from './components/sidebar/sidebar.component'
import { TimelineComponent } from './components/timeline/timeline.component'
import { DayViewComponent } from './components/day-view/day-view.component'
import { WeekViewComponent } from './components/week-view/week-view.component'
import { EventDetailComponent } from './components/event-detail/event-detail.component'

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [
    MatProgressBarModule,
    MatSnackBarModule,
    HeaderComponent,
    SidebarComponent,
    TimelineComponent,
    DayViewComponent,
    WeekViewComponent,
    EventDetailComponent,
  ],
  templateUrl: './app.component.html',
  styleUrl: './app.component.scss',
})
export class AppComponent implements OnInit {
  protected readonly activity = inject(ActivityService)
  protected readonly theme = inject(ThemeService)
  private readonly snackBar = inject(MatSnackBar)
  private snackRef: MatSnackBarRef<TextOnlySnackBar> | null = null
  private activeErrorMessage: string | null = null

  constructor() {
    effect(() => {
      const error = this.activity.error()

      if (!error) {
        this.snackRef?.dismiss()
        this.snackRef = null
        this.activeErrorMessage = null
        return
      }

      if (this.activeErrorMessage === error) {
        return
      }

      this.snackRef?.dismiss()
      this.activeErrorMessage = error
      this.snackRef = this.snackBar.open(error, 'Dismiss', {
        horizontalPosition: 'center',
        verticalPosition: 'bottom',
        panelClass: ['error-alert-snackbar'],
      })
    })
  }

  ngOnInit(): void {
    void this.activity.initLoad()
  }
}
