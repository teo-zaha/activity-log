import { Component, OnInit, inject } from '@angular/core'
import { MatProgressBarModule } from '@angular/material/progress-bar'
import { ActivityService } from './services/activity.service'
import { ThemeService } from './services/theme.service'
import { HeaderComponent } from './components/header/header.component'
import { FiltersComponent } from './components/filters/filters.component'
import { TimelineComponent } from './components/timeline/timeline.component'
import { DayViewComponent } from './components/day-view/day-view.component'
import { WeekViewComponent } from './components/week-view/week-view.component'
import { EventDetailComponent } from './components/event-detail/event-detail.component'

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [
    MatProgressBarModule,
    HeaderComponent,
    FiltersComponent,
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
  // Inject ThemeService to ensure it initializes and applies saved preference
  protected readonly theme = inject(ThemeService)

  ngOnInit(): void {
    void this.activity.initLoad()
  }
}
