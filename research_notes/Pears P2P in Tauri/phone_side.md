# Phone side of a Holepunch/Pears P2P link to a Tauri desktop app (iOS and Android)

All sources were accessed on 2026-11-03. Repo, release and npm dates come from the GitHub API (`gh api repos/holepunchto/...`) and `npm view`. "README" means the repo's default-branch README on that date.

## 1. Bare Kit: what it provides on iOS and Android, worklets, IPC, bundling, architectures, size, maturity, examples

### Takeaway

Bare Kit is Holepunch's official way to run the Bare JS runtime (V8-based) inside a native iOS or Android app. Each JS entry point runs as a "worklet" on its own thread in the app process, with a byte-stream IPC to the host. It changes very fast: v3.0.0 (2026-10-30) turned bare-kit into a plain C library and moved the language bindings into separate repos (`bare-kit-swift` v2.0.0, `bare-kit-kotlin` v0.2.0, both released 2026-10-30 to 2026-11-02). `bare-stow` (created 2026-06, v0.2.0) is now the tool that builds bundles and generates harnesses. Expect breaking churn.

### Cited Findings

**Core concept and lifecycle (bare-kit v3, C library)**

- bare-kit is "a C library that embeds Bare in a host application". Each JS entry point runs in an isolated thread called a **worklet**, with an in-process IPC channel to the host. It is meant as a general embedder for language bindings to build on. — [bare-kit README](https://github.com/holepunchto/bare-kit)
- v3.0.0 (published 2026-10-30) changelog: "Rework as a standalone C library", "Swap worklet IPC from pipes to the in-process queue", "Rework IPC lifecycle", "Fix suspension race". v3.0.1 followed the same day. The previous line was v2.5.5 (2026-09-18). — [bare-kit v3.0.0 release](https://github.com/holepunchto/bare-kit/releases/tag/v3.0.0); [releases](https://github.com/holepunchto/bare-kit/releases)
- At v2.5.5 the bare-kit repo itself contained `apple/` and `android/` directories with Objective-C (`BareWorklet`, `BareIPC`) and Java (`to.holepunch.bare.kit.Worklet`, `IPC`) APIs. At v3 the tree has only `include/`, `src/`, `test/` and `CMakeLists.txt`. — [bare-kit v2.5.5 README](https://github.com/holepunchto/bare-kit/blob/v2.5.5/README.md); [bare-kit main tree](https://github.com/holepunchto/bare-kit)
- Worklet lifecycle: `bare_worklet_start()` runs on its own thread. The worklet exits when its event loop drains, when it calls `Bare.exit()`, or when the host terminates it. An open IPC keeps it alive. — [bare-kit README](https://github.com/holepunchto/bare-kit)
- Suspension: `bare_worklet_suspend/resume/wakeup()` forward host lifecycle changes as `suspend`, `resume` and `wakeup` events on `Bare`. A suspension can "linger" for some milliseconds, and on iOS "during suspension linger, the worklet holds a background task". — [bare-kit README](https://github.com/holepunchto/bare-kit)
- Uncaught exceptions or unhandled rejections in a worklet **abort the whole host process**. To avoid this, register `uncaughtException`/`unhandledRejection` handlers in the worklet and report the error over IPC. — [bare-kit README](https://github.com/holepunchto/bare-kit); [v2.5.5 README](https://github.com/holepunchto/bare-kit/blob/v2.5.5/README.md)
- Native addons resolve Bare's symbols from the host process. They "may bind to the wrong symbols if the process already exports libuv, V8, or Node-API (as Node.js and Electron do), or if it has more than one copy of Bare Kit". — [bare-kit README](https://github.com/holepunchto/bare-kit)
- Resource paths: on iOS a relative filename resolves against the app bundle's resources, and app extensions share them. On Android it resolves against the code-cache dir, and assets listed in `<name>.assets` are extracted from the APK before first run. — [bare-kit README](https://github.com/holepunchto/bare-kit)

**IPC**

- Worklet side: a duplex stream `Bare.IPC`. Host side (C): non-blocking `bare_ipc_read/write()` with poll callbacks that run on an IPC-owned thread. Half-close is supported. — [bare-kit README](https://github.com/holepunchto/bare-kit)
- Swift binding (`bare-kit-swift`, SPM, v2.0.0, 2026-10-30): `let worklet = try Worklet(); let ipc = try IPC(worklet: worklet); try worklet.start("app.bundle"); try await ipc.write(...); for await data in ipc { ... }`. Releasing the worklet terminates it. Addons linked with `bare-link` are loaded by the system linker and embedded as frameworks. Package.swift declares `platforms: [.macOS(.v13), .iOS(.v15)]` and pulls a binary `BareKit.xcframework.zip`. — [bare-kit-swift README](https://github.com/holepunchto/bare-kit-swift); [Package.swift](https://github.com/holepunchto/bare-kit-swift/blob/main/Package.swift)
- Kotlin binding (`bare-kit-kotlin`, created 2026-10-27, v0.2.0 on 2026-11-02, via JitPack): `val worklet = Worklet(context); val ipc = Ipc(worklet); worklet.start("app.bundle"); ipc.write(...); ipc.data.collect { ... }`. It supports Android plus the JVM on Linux, macOS and Windows. The Android app context is passed to addons as `bare.android.context.v1`. Addons go in `jniLibs`. — [bare-kit-kotlin README](https://github.com/holepunchto/bare-kit-kotlin)
- Typed RPC over the IPC: `bare-rpc` handles framing. `hyperschema` generates `compact-encoding` codecs. A Swift toolchain (`hyperschema-swift`, `bare-rpc-swift`, `compact-encoding-swift`, `hrpc-swift`) generates Swift structs and a typed RPC class. C and **Kotlin generators are "following", so they are not yet available**. Patterns: unary, send-only, request/response streams, duplex. — [Pear docs, "One core, many platforms"](https://docs.pears.com/explanation/bare-on-native/)
- bare-rpc is at npm v1.4.0 (modified 2026-11-03). — `npm view bare-rpc`

**Bundling (bare-pack -> bare-stow)**

- `bare-stow` (repo created 2026-06-09, npm v0.2.0 on 2026-11-02) "packs a module graph into a bundle with bare-pack and generates a harness package that boots the bundle from a host application written in JavaScript, Kotlin, Python, or Swift". The entry exports `start(ipc, ready)`. Asking for an RPC `server` gives `{ rpc, ipc }`. — [bare-stow README](https://github.com/holepunchto/bare-stow)
- bare-stow targets: `bare-kit` (worklet, `swift` on macOS/iOS, `kotlin` on Android/JVM). `bare-sidecar` (separate process, `javascript`/`python`/`swift` on desktop). `bare-worker` (thread in a Bare host, JS). Native addons are linked with `bare-link`. With `linking=app`, one `bare-stow link` call links the addons and assets of all bundles once. — [bare-stow README](https://github.com/holepunchto/bare-stow)
- Example: `bare-stow pack --target bare-kit --target-option linking=app --language swift --out ./ios/Core ./core.js`. — [bare-stow README](https://github.com/holepunchto/bare-stow)
- `bare-pack` is still the underlying bundler (npm v3.0.2, 2026-10-20). — `npm view bare-pack`; [bare-stow README](https://github.com/holepunchto/bare-stow)

**Architectures, OS versions, engine**

- Bare platform tiers. Tier 1: Android arm, arm64, ia32, x64 on **Android >= 10**. iOS arm64 on **iOS >= 15.0**. iOS x64 (simulator only). macOS >= 13 (arm64/x64). Windows, Linux. — [Bare README, Platform support](https://github.com/holepunchto/bare#platform-support)
- `react-native-bare-kit` sets `minSdk 29` (Android 10) in its own Gradle config. — [pear-mobile README](https://github.com/holepunchto/pear-mobile)
- **iOS runs V8 jitless.** V8 is built with the DrumBrake WebAssembly interpreter on all targets except Android, which "allows WebAssembly to run with `--jitless`, which is required on iOS". — [chromium-prebuilds README](https://github.com/holepunchto/chromium-prebuilds); the macOS example says the allow-jit entitlement "is inert on iOS, where Bare runs V8 jitless" — [bare-macos README](https://github.com/holepunchto/bare-macos)
- libjs (Bare's engine layer) passes `--no-freeze-flags-after-init` so that V8 does not call `mprotect()`, "which isn't allowed on iOS in unprivileged processes". It maps options to `--lite-mode` (optimize for memory) or `--jitless`. — [libjs src/js.cc](https://github.com/holepunchto/libjs/blob/main/src/js.cc)
- **iOS entitlement required.** V8 pointer compression plus the cppgc caged heap reserve more virtual memory than the default iOS app address space allows ("Failed to reserve virtual memory for process-wide V8 pointer compression cage"). Apps that link the iOS prebuilds must be signed with `com.apple.developer.kernel.extended-virtual-addressing`. — [chromium-prebuilds README](https://github.com/holepunchto/chromium-prebuilds)
- "A JavaScriptCore variant also ships in the release" (of BareKit) alongside the default V8 build. — [bare-macos README](https://github.com/holepunchto/bare-macos)

**Binary size (download artifacts; not installed app size)**

- `BareKit.xcframework.zip` (bare-kit-swift v2.0.0): 55,278,451 bytes (~55 MB, compressed, multi-slice). — [bare-kit-swift release v2.0.0](https://github.com/holepunchto/bare-kit-swift/releases/tag/v2.0.0)
- `bare-kit-kotlin` v0.2.0 `prebuilds.zip`: 147,083,911 bytes (~147 MB). Per its README it contains the Android ABIs plus JVM desktop builds. — [bare-kit-kotlin releases](https://github.com/holepunchto/bare-kit-kotlin/releases); [README](https://github.com/holepunchto/bare-kit-kotlin)
- bare-kit v2.5.5 all-platform `prebuilds.zip`: ~418 MB. v2.1.3: ~315 MB. — [bare-kit releases](https://github.com/holepunchto/bare-kit/releases)

**Maturity and examples**

- Activity: bare-kit has 46 stars, was created 2024-06-21 and last pushed 2026-10-30. Bare runtime v1.36.2 (2026-10-30), 1,249 stars. — GitHub API ([bare-kit](https://github.com/holepunchto/bare-kit), [bare](https://github.com/holepunchto/bare))
- Official example apps: [bare-ios](https://github.com/holepunchto/bare-ios), [bare-android](https://github.com/holepunchto/bare-android), [bare-expo](https://github.com/holepunchto/bare-expo), [bare-macos](https://github.com/holepunchto/bare-macos), bare-linux, bare-windows. bare-ios is a "P2P shared switch" that runs the same `bare-switch-core` as bare-macos. It also runs a second worklet in a push-notification service extension, and its build steps still pull bare-kit **v2.1.3** `prebuilds.zip` and use `bare-link` plus packing in Xcode scheme pre-actions. — [bare-ios README](https://github.com/holepunchto/bare-ios)
- Experimental Holepunch repos: `bare-ui-kit` ("UIKit bindings and runtime for Bare"), `bare-web-kit` ("WebKit bindings for Bare"), `bare-app-kit`, `bare-kit-python`. — [GitHub search, holepunchto org](https://github.com/orgs/holepunchto/repositories?q=bare-kit)

### Inferences

- Holepunch's docs and examples lag behind the code. The Pear docs page still says "drive worklets from Objective-C / Java". The current supported path is `bare-kit-swift` / `bare-kit-kotlin` plus `bare-stow`, and the v2 to v3 change happened about four days before this note. Pin exact versions.
- The 55 MB xcframework zip probably holds iOS device, iOS simulator and macOS slices. The per-architecture contribution to a shipped IPA/AAB is likely much smaller, but it was not measured (see Gaps).
- Because Swift typed-RPC codegen exists and Kotlin codegen does not, Android hosts need hand-written framing or should keep RPC in JS. A webview or React Native UI that talks JS-to-JS avoids the problem entirely.

### Gaps

- No published per-ABI installed-size numbers for an app embedding Bare Kit plus Hyperswarm.
- No documented benchmark of jitless V8 on iOS for Hyperswarm/Noise/crypto workloads. Crypto runs in native addons (sodium), so the JS-JIT penalty is probably modest for this use case, but no source measures it.
- Whether the JavaScriptCore variant is shipped for iOS and is production-ready was not confirmed.

## 2. React Native integration: react-native-bare-kit, expo-bare-kit, pear-mobile, Keet

### Takeaway

React Native (and Expo with prebuild/dev builds) is Holepunch's own first-class mobile path. `react-native-bare-kit` (v0.17.1, 2026-11-03) exposes `Worklet`/`IPC` to RN JS. `expo-bare-kit` (v0.2.0, 2026-11-02) is a config plugin that packs workers during Metro bundling, links addons, and can wire push-notification worklets. `pear-mobile` (v4.3.0, 2026-08-18) adds Pear's storage and OTA updater but is labelled "MVP - EXPERIMENTAL".

### Cited Findings

- `react-native-bare-kit` is a TurboModule (codegen `BareKitSpec`, `RCTTurboModule`), so it needs the RN New Architecture. Peer deps `react`/`react-native` `*`. Version 0.17.1. — [package.json](https://github.com/holepunchto/react-native-bare-kit/blob/main/package.json); [releases](https://github.com/holepunchto/react-native-bare-kit/releases)
- API: `new Worklet()`, `worklet.start('/app.js', source)`, `worklet.IPC.on('data')` / `.write()`. "Bare Kit isn't included": the app must supply Bare Kit plus the addons and assets. Workers are packed with `bare-stow pack --target react-native` and linked with `bare-stow link --target bare-kit --language swift|kotlin`. `console.*` in the worklet goes to the system log under the `bare` identifier. — [react-native-bare-kit README](https://github.com/holepunchto/react-native-bare-kit)
- `expo-bare-kit` is an Expo config plugin: list `workers` in app.json, wrap Metro with `expo-bare-kit/metro`, run `npx expo prebuild`, then `import core from 'expo-bare-kit/workers/core'; const { ipc } = await core.start()`. It can set up iOS/Android notification worker extensions (Android needs `google-services.json`). — [expo-bare-kit README](https://github.com/holepunchto/expo-bare-kit)
- Expo example: [bare-expo](https://github.com/holepunchto/bare-expo) ("Example of embedding Bare in an Expo application"), last pushed 2026-10-28.
- `pear-mobile`: "Embeddable Pear runtime for mobile applications. Provides storage path and bare workers via worklets." It is "MVP and Experimental", supports Android and iOS, and requires `react-native-bare-kit` as a direct root dependency (because of autolinking) and minSdk 29. OTA bundle boot is in `pear-runtime-react-native`. Quick start: `hello-pear-react-native` (pushed 2026-10-22). — [pear-mobile README](https://github.com/holepunchto/pear-mobile); [hello-pear-react-native](https://github.com/holepunchto/hello-pear-react-native)
- Pear docs: `pear-runtime` targets desktop. The mobile counterpart is `pear-mobile`, or you "wire the worklet and updates yourself with bare-kit". The same JS core runs in a desktop worker, a terminal binary or a mobile worklet. — [Pear docs, bare-on-native](https://docs.pears.com/explanation/bare-on-native/)
- Keet: the Pear docs only say this architecture "underlies Keet's consistent behaviour across phones, laptops, and terminals". They do not describe the mobile implementation. — [Pear docs, bare-on-native](https://docs.pears.com/explanation/bare-on-native/)
- The `docs.pears.com/examples/bare-on-mobile` page linked from search results returned HTTP 404 on 2026-11-03. — [docs.pears.com/examples/bare-on-mobile](https://docs.pears.com/examples/bare-on-mobile)
- Community Flutter wrappers (`flutter_pear`, `flutter_pear_bare`, v0.3.x) wrap Bare Kit. A search-result summary says iOS support is validated on the simulator only. — [pub.dev flutter_pear](https://pub.dev/packages/flutter_pear/changelog) (not independently verified; treat as community and low maturity)

### Inferences

- Expo Go cannot load custom native code, so Expo use means a development build or prebuild (implied by "run prebuild" in the expo-bare-kit README).
- An RN/Expo app could not reuse the Angular components directly. Reuse would mean embedding the Angular build in a `react-native-webview` and bridging RN to the webview with `postMessage`, which adds a hop compared with Option 3/4 below.

### Gaps

- No public source documents Keet mobile's internal architecture (RN or not, number of worklets, background strategy).
- Expo SDK version compatibility of expo-bare-kit 0.2.0 is not stated.

## 3. Tauri 2 mobile hosting a Bare Kit worklet plus the Angular webview UI

### Takeaway

No example of Tauri mobile plus Bare Kit was found. However, Tauri 2 mobile plugins are Swift packages (iOS) and Kotlin/Java Gradle modules (Android), and Bare Kit now ships exactly those: an SPM package and a JitPack Gradle artifact. That makes a Tauri mobile plugin wrapping a worklet technically plausible. The Angular dashboard would run unchanged in the Tauri webview, and `trigger()` events plus `invoke('plugin:...')` commands would carry IPC bytes or JSON. Known friction points: Android minSdk 29, the iOS extended-virtual-addressing entitlement, and Android plugin commands running on the main thread.

### Cited Findings

- Tauri 2 mobile plugins: Android uses a Kotlin class extending `app.tauri.plugin.Plugin` with `@TauriPlugin`, and `@Command fun x(invoke: Invoke)` with `invoke.resolve(...)`. iOS uses a Swift class extending `Plugin`, with `@objc` functions taking `Invoke`, and the plugin "is a Swift package". JS calls `invoke('plugin:<name>|<cmd>')`. Rust calls `run_mobile_plugin(...)`. Plugins push events with `trigger("event", data)`, received in JS via `addPluginListener(...)` (needs a capability permission). There are `load()` lifecycle hooks and `onNewIntent` on Android. Docs last updated 2026-05-14. — [Tauri: Mobile Plugin Development](https://v2.tauri.app/develop/plugins/develop-mobile/)
- "On Android native commands are scheduled on the main thread". Long-running work must use coroutines (`Dispatchers.IO`) to avoid ANRs. — [Tauri mobile plugin docs](https://v2.tauri.app/develop/plugins/develop-mobile/)
- Rust-to-native on mobile uses JNI (Android) or C FFI (iOS), with manual naming and memory management. — [Tauri mobile plugin docs](https://v2.tauri.app/develop/plugins/develop-mobile/)
- `bare-kit-swift` is an SPM package (`.product(name: "BareKit", package: "bare-kit-swift")`, iOS 15+). `bare-kit-kotlin` is a JitPack Gradle dependency with a coroutine `Flow`-style `ipc.data.collect`. — [bare-kit-swift](https://github.com/holepunchto/bare-kit-swift); [bare-kit-kotlin](https://github.com/holepunchto/bare-kit-kotlin)
- bare-stow can generate the Swift package and Kotlin Gradle module harness for a bundle (`--target bare-kit --language swift|kotlin`). No Rust language target is listed (only JavaScript, Kotlin, Python, Swift). — [bare-stow README](https://github.com/holepunchto/bare-stow)
- The bare-kit symbol-conflict warning names processes that already export libuv, V8 or Node-API, or that contain two copies of Bare Kit. — [bare-kit README](https://github.com/holepunchto/bare-kit)
- QR scanning in Tauri: the official `@tauri-apps/plugin-barcode-scanner` supports Android and iOS. Call `scan({ windowed: true, formats: [Format.QRCode] })`. iOS needs `NSCameraUsageDescription` in `Info.ios.plist`, and capabilities must allow `barcode-scanner:allow-scan`. In windowed mode the webview turns transparent over the camera. Requires Rust >= 1.90. Page last updated 2025-07-01. — [Tauri barcode-scanner plugin](https://v2.tauri.app/plugin/barcode-scanner/)

### Inferences

- **Likely architecture:** create a Tauri mobile plugin (Swift + Kotlin). In `load()`, start a worklet from a bare-stow-generated harness. Forward `ipc` data to the webview via `trigger("ipc-data", base64/JSON)`, and expose a `write` command. The Angular service layer would swap `HttpClient` calls for `invoke`/`addPluginListener`. This plugs into the same Angular service abstraction the desktop Tauri build already needs.
- Tauri mobile uses system WKWebView / Android WebView, not an embedded V8, so the libuv/V8 symbol-conflict warning probably does not apply. It still needs testing, especially if the Rust side links other C libraries that export libuv.
- Android minSdk must be raised to 29 to match Bare's Android >= 10 tier. Tauri's default Android minSdk was not checked (see Gaps).
- A Rust-native Holepunch stack on the phone (no Bare) would avoid JS entirely, but no maintained Rust implementation of the current HyperDHT/Hyperswarm protocol was researched here. Treat it as unknown.
- Shipping with Tauri means maintaining Swift and Kotlin glue that no Holepunch example covers. RN/Expo is the only officially exemplified mobile host.

### Gaps

- No public example or issue was found that combines Tauri mobile and Bare Kit. Whether SPM binary targets inside a Tauri iOS plugin package build cleanly was not verified.
- Tauri's default Android `minSdkVersion` and how to raise it were not checked in this pass.
- Throughput and latency of passing activity data through Tauri plugin events (JSON serialization over the webview bridge) were not measured.

## 4. Capacitor/Ionic or a plain WebView wrapper with a native Bare Kit worklet

### Takeaway

This is feasible with the same pattern as Tauri: a custom Capacitor plugin in Swift/Kotlin that owns the worklet and bridges IPC to the Angular app. Capacitor is Angular-native territory, so UI reuse is straightforward. No existing Capacitor plus Bare Kit plugin was found. A plain WKWebView/Android WebView wrapper with `WKScriptMessageHandler`/`addJavascriptInterface` also works but means hand-rolling the bridge.

### Cited Findings

- Capacitor v8 docs: custom native iOS code is a Swift class subclassing `CAPPlugin`, conforming to `CAPBridgedPlugin` with `identifier`, `jsName` and `pluginMethods`. `@objc` methods take `CAPPluginCall` and return via `call.resolve(...)`. It is registered with `bridge?.registerPluginInstance(...)` in a custom view controller. JS uses `registerPlugin<T>('Name')` from `@capacitor/core` with a TypeScript interface. — [Capacitor: Custom Native iOS Code](https://capacitorjs.com/docs/ios/custom-code)
- A web search for an existing Hyperswarm/Bare Kit Capacitor plugin returned nothing relevant (2026-11-03). — [search performed; no result URL to cite]
- Holepunch has experimental `bare-web-kit` ("WebKit bindings for Bare") and `bare-ui-kit` ("UIKit bindings and runtime for Bare"). These point to a possible future "all-Bare" app hosting a WebKit view, but neither is documented for production. — [holepunchto repos](https://github.com/orgs/holepunchto/repositories?q=bare-kit)

### Inferences

- Capacitor vs Tauri for this app: Capacitor has a larger plugin ecosystem (camera/ML Kit barcode plugins, background tasks) and is purpose-built for Angular/Ionic. Tauri keeps one toolchain with the desktop app. Both need the same custom native worklet plugin. With Capacitor, the Rust `core` crate would not be on the phone, which is fine because the phone only displays data.
- Typed contract: the generated TS DTOs (`dashboard/src/app/generated/`) could be reused on the phone if the worklet just relays JSON payloads produced by the desktop. The worklet then only acts as a transport, not as business logic.

### Gaps

- Whether Capacitor's plugin template supports SPM binary dependencies (needed for `bare-kit-swift`) versus CocoaPods only was not confirmed. The docs page fetched did not cover it.
- Capacitor `notifyListeners` event semantics for streaming IPC were not verified in this pass.

## 5. Platform constraints: background execution, UDP and local network permission, App Store JS-runtime rules, JIT, Android battery

### Takeaway

iOS: Bare runs V8 **jitless** on iOS, as Apple requires. It needs the extended-virtual-addressing entitlement. Shipping the JS bundle inside the app is consistent with Guideline 2.5.2, but OTA code updates (pear-mobile's updater) fall in 2.5.2's grey zone. Connecting to a desktop on the same Wi-Fi triggers the iOS Local Network prompt (`NSLocalNetworkUsageDescription`). Neither OS lets the P2P socket live in the background indefinitely. Design for "connect while the app is in the foreground". Android Doze and App Standby suspend network access when the device is idle.

### Cited Findings

- Apple 2.5.2: apps "may not ... download, install, or execute code which introduces or changes features or functionality of the app". — [App Store Review Guidelines](https://developer.apple.com/app-store/review/guidelines/)
- Apple 4.7 permits HTML5/JS mini apps not embedded in the binary, under the 4.7.1 to 4.7.5 rules (privacy, no exposing native APIs without permission, and so on). — [App Store Review Guidelines](https://developer.apple.com/app-store/review/guidelines/)
- Apple 2.5.4: "Multitasking apps may only use background services for their intended purposes: VoIP, audio playback, location, task completion, local notifications, etc." — [App Store Review Guidelines](https://developer.apple.com/app-store/review/guidelines/)
- iOS JIT: Bare runs V8 jitless on iOS. `--jitless` is "required on iOS". `mprotect()` "isn't allowed on iOS in unprivileged processes". — [bare-macos README](https://github.com/holepunchto/bare-macos); [chromium-prebuilds README](https://github.com/holepunchto/chromium-prebuilds); [libjs js.cc](https://github.com/holepunchto/libjs/blob/main/src/js.cc)
- iOS entitlement `com.apple.developer.kernel.extended-virtual-addressing` is required for the V8 pointer-compression cage on devices. — [chromium-prebuilds README](https://github.com/holepunchto/chromium-prebuilds)
- **iOS Local Network privacy (TN3179):** this applies to traffic to a "local network address" (Wi-Fi/Ethernet, not cellular or VPN). These operations require permission: outgoing TCP connection, sending UDP unicast, multicast or broadcast, and connecting a UDP socket. Receiving incoming UDP unicast and accepting incoming TCP do **not** require it. The first access shows an alert and the decision is remembered. Apps should add `NSLocalNetworkUsageDescription`. Multicast on iOS needs the `com.apple.developer.networking.multicast` entitlement. Supported on iOS 14+ and macOS 15+. An iOS 18 bug (FB14321888) desynced the permission state and was fixed in iOS 18.6. — [TN3179: Understanding local network privacy](https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy)
- Bare Kit's suspension API (`suspend` with linger, holding an iOS background task during linger) is the mechanism for cooperating with OS lifecycle. — [bare-kit README](https://github.com/holepunchto/bare-kit); [v2.5.5 README](https://github.com/holepunchto/bare-kit/blob/v2.5.5/README.md)
- expo-bare-kit and bare-ios support a separate worklet in a **push-notification service extension**, which is Holepunch's pattern for background wake-ups. — [expo-bare-kit README](https://github.com/holepunchto/expo-bare-kit); [bare-ios README](https://github.com/holepunchto/bare-ios)
- **Android Doze:** when the device is unplugged, stationary and has the screen off, Doze "suspends network access", ignores wake locks, and defers jobs, syncs and alarms to maintenance windows. App Standby defers network for idle apps (idle apps get network about once a day if idle for long). Foreground services or activities prevent standby. Google recommends FCM. Play policy prohibits requesting direct battery-optimization exemption unless the app's core function is broken. — [Android: Optimize for Doze and App Standby](https://developer.android.com/training/monitoring-device-state/doze-standby)
- Android: Bare requires Android 10+ (API 29). — [Bare README](https://github.com/holepunchto/bare#platform-support); [pear-mobile README](https://github.com/holepunchto/pear-mobile)

### Inferences

- **Holepunch connections on the same LAN trigger the iOS prompt.** HyperDHT holepunching sends UDP to the peer's addresses. When phone and desktop share a Wi-Fi network, those include private LAN addresses, which TN3179 says require local network access. Add `NSLocalNetworkUsageDescription` and handle denial (the connection may still succeed via public addresses or relays, at worse latency). Whether HyperDHT advertises LAN addresses was not verified in its source.
- Internet UDP needs no special permission on either OS. Android needs only `android.permission.INTERNET` (standard; not re-verified here).
- For an "activity viewer" phone app, the realistic model is: open the app, the worklet resumes, the swarm reconnects, data syncs, and the worklet suspends on background. Persistent background P2P is neither feasible nor needed. If data should be available offline, the phone should persist the last snapshot (e.g., in a Hypercore/Hyperbee in the worklet, or app storage).
- App Store risk is low if the worklet bundle is compiled into the app and data (not code) flows over P2P. Using pear-mobile's OTA updater to change app JS would need review against 2.5.2. The Apple Developer Program License Agreement clause on interpreted code was not fetched.

### Gaps

- No measurements of how long iOS keeps a Bare worklet's UDP socket usable after backgrounding.
- Apple Developer Program License Agreement section 3.3 text on downloaded interpreted code was not retrieved.
- Android 14+ foreground-service type requirements (e.g., `dataSync`) for a long-lived P2P service were not researched.

## 6. Camera/QR scanning options per stack

### Takeaway

Every stack has a maintained QR path. Tauri has the official barcode-scanner plugin. RN/Expo has expo-camera barcode scanning (not verified in this pass). Capacitor has community or official barcode plugins (not verified). Native Swift/Kotlin can use AVFoundation / CameraX + ML Kit (not verified). The QR payload would carry the desktop's public key or topic plus any pairing secret.

### Cited Findings

- Tauri: `@tauri-apps/plugin-barcode-scanner`, mobile only (`#[cfg(mobile)]`), `scan({ windowed: true, formats: [Format.QRCode] })`, iOS `NSCameraUsageDescription`, capability permissions `barcode-scanner:allow-scan`/`allow-cancel`. Default permission set includes `check-permissions`, `request-permissions` and `open-app-settings`. — [Tauri barcode-scanner](https://v2.tauri.app/plugin/barcode-scanner/)

### Inferences

- With Tauri, the scanner and the Bare worklet plugin live side by side as two mobile plugins. Angular calls `scan()`, then `invoke('plugin:bare|connect', { key })`.
- The OS camera app can also scan a QR containing a custom-scheme or universal link (e.g., `activitylog://pair?key=...`) that deep-links into the app. Tauri's `onNewIntent` hook on Android supports deep links. — derived from [Tauri mobile plugin docs](https://v2.tauri.app/develop/plugins/develop-mobile/)

### Gaps

- Expo (`expo-camera` barcode scanning), Capacitor (`@capacitor-mlkit/barcode-scanning` or similar) and native ML Kit/AVFoundation options were not fetched or verified in this pass.

## 7. Can a mobile browser join without an app (no UDP)? Relay requirement

### Takeaway

Not directly. Browsers cannot open raw UDP sockets, so a browser cannot be a HyperDHT peer. Holepunch's browser bridge, `@hyperswarm/dht-relay`, relays the DHT over framed streams from a non-browser node, but the repo is **archived** and labelled "experimental, do not use in production". A browser-only phone client therefore needs a relay you operate (or the desktop serving the UI over HTTPS/WebSocket on the LAN), which gives up the "no server" property.

### Cited Findings

- `holepunchto/hyperswarm-dht-relay` is `archived: true` (last push 2025-11-01). The README warns: "This project is still experimental. Do not use it in production". The relaying side runs `relay(new DHT(), stream)` and the relayed side runs `new DHT(stream)` with the same API as hyperdht. It provides stream wrappers for common transports (TCP shown). — [hyperswarm-dht-relay](https://github.com/holepunchto/hyperswarm-dht-relay)
- npm `@hyperswarm/dht-relay` latest is 0.4.3 (metadata modified 2026-03-31). — `npm view @hyperswarm/dht-relay`

### Inferences

- A browser-only client would need: (a) a WebSocket-reachable relay node running HyperDHT, which is a server, or (b) the desktop app exposing a LAN WebSocket/HTTPS endpoint (local-only, needs a TLS/cert story for secure contexts). Either way it is no longer pure Holepunch P2P. The relay sees encrypted Noise streams if end-to-end secret-stream is preserved, but it is still infrastructure.
- Browsers lack raw UDP sockets (well-known platform constraint; WebTransport/WebRTC are the only P2P-ish options and are not HyperDHT-compatible). No specific source fetched.

### Gaps

- Whether `dht-relay` has a maintained successor (e.g., a WebSocket relay used by Keet web or Pear) was not found.
- Whether the relay package still ships a WebSocket transport wrapper was not verified (only the TCP section of the README was read).
