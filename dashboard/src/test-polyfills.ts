// Loaded before Angular in the specs only (`build:test`, the unit-test builder's build target).
//
// jsdom has no Web Animations, and Angular decides once, as it loads, whether it runs
// `animate.enter`/`animate.leave`: only where `getAnimations` exists. With this the specs see the
// app's enter and leave callbacks as a browser does; nothing animates, so each completes at once.
if (typeof Element.prototype.getAnimations !== 'function') {
  Element.prototype.getAnimations = (): Animation[] => []
}
