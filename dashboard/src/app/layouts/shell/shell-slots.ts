import {
  Directive,
  Service,
  type Signal,
  TemplateRef,
  type WritableSignal,
  effect,
  inject,
  input,
  signal,
} from '@angular/core'

/** The places in the shell a layout under it fills: the header's row and the sidebar. */
export type ShellSlotName = 'header' | 'sidebar'

/**
 * What the layout on screen shows in the shell's header row and sidebar, such as the views'
 * range and source chips. The shell provides it and renders each slot where it belongs.
 *
 * Only layouts fill slots, never a view: the router keeps the timeline detached while another
 * view shows (`TimelineReuseStrategy`), without destroying it, so a slot it filled would stay.
 */
@Service({ autoProvided: false })
export class ShellSlots {
  private readonly slots: Readonly<
    Record<ShellSlotName, WritableSignal<TemplateRef<unknown> | null>>
  > = { header: signal(null), sidebar: signal(null) }

  template(name: ShellSlotName): Signal<TemplateRef<unknown> | null> {
    return this.slots[name].asReadonly()
  }

  fill(name: ShellSlotName, template: TemplateRef<unknown>): void {
    this.slots[name].set(template)
  }

  /** Empties the slot if it still shows `template`; a layout that came since keeps its own. */
  clear(name: ShellSlotName, template: TemplateRef<unknown>): void {
    this.slots[name].update(current => (current === template ? null : current))
  }
}

/**
 * `<app-range-form *shellSlot="'header'" />`: the element shows in the shell's slot instead of
 * here, for as long as the layout that declares it lives.
 */
@Directive({ selector: '[shellSlot]' })
export class ShellSlotDirective {
  readonly shellSlot = input.required<ShellSlotName>()

  constructor() {
    const template = inject<TemplateRef<unknown>>(TemplateRef)
    const slots = inject(ShellSlots)
    effect(onCleanup => {
      const name = this.shellSlot()
      slots.fill(name, template)
      onCleanup(() => slots.clear(name, template))
    })
  }
}
