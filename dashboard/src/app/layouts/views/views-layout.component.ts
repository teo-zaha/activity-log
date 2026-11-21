import { Component, ViewEncapsulation, inject } from '@angular/core'
import { RouterOutlet } from '@angular/router'
import { CollectionStatusComponent } from '../../components/collection-status/collection-status.component'
import { NoSourcesComponent } from '../../components/no-sources/no-sources.component'
import { RangeFormComponent } from '../../components/range-form/range-form.component'
import { SourceListComponent } from '../../components/source-list/source-list.component'
import { ActivityDataService } from '../../services/activity-data.service'
import { ShellSlotDirective } from '../shell/shell-slots'

/**
 * The activity views' layout: the collection range and Collect in the header, the source chips
 * in the sidebar, the collection's status, and the view, or what to do without sources.
 */
@Component({
  selector: 'app-views-layout',
  imports: [
    CollectionStatusComponent,
    NoSourcesComponent,
    RangeFormComponent,
    RouterOutlet,
    ShellSlotDirective,
    SourceListComponent,
  ],
  templateUrl: './views-layout.component.html',
  styleUrl: './views-layout.component.scss',
  encapsulation: ViewEncapsulation.None,
})
export class ViewsLayoutComponent {
  protected readonly data = inject(ActivityDataService)
}
