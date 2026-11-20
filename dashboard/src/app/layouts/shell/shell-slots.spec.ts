import { Component, TemplateRef, viewChild } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { describe, expect, it } from 'vitest'
import { ShellSlots } from './shell-slots'

@Component({ template: '<ng-template #a>a</ng-template><ng-template #b>b</ng-template>' })
class Templates {
  readonly a = viewChild.required<TemplateRef<unknown>>('a')
  readonly b = viewChild.required<TemplateRef<unknown>>('b')
}

describe('ShellSlots', () => {
  it('lets a layout that left empty only the slot it still holds', () => {
    TestBed.configureTestingModule({ providers: [ShellSlots] })
    const slots = TestBed.inject(ShellSlots)
    const fixture = TestBed.createComponent(Templates)
    fixture.detectChanges()
    const [leaving, coming] = [fixture.componentInstance.a(), fixture.componentInstance.b()]

    slots.fill('header', leaving)
    slots.fill('header', coming)
    slots.clear('header', leaving)
    expect(slots.template('header')()).toBe(coming)

    slots.clear('header', coming)
    expect(slots.template('header')()).toBeNull()
  })
})
