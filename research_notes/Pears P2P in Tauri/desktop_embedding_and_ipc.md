# Running Bare/Pear P2P JS (Hyperswarm, HyperDHT, blind-pairing, secret-stream) from a Tauri 2 desktop app: embedding options, packaging and IPC

Research date: 2026-11-03. Versions checked on npm/GitHub that day: bare 1.36.2 (GitHub release 2026-10-30), bare-runtime 1.36.0 (2026-10-21), bare-kit v3.0.1 (tag 2026-10-30), bare-sidecar 0.5.14 (2026-11-02), bare-build 1.6.2 (2026-10-30), bare-pack 3.0.2, bare-rpc 1.4.0 (2026-11-03), bare-ipc 1.1.3, pear-runtime 1.3.1 (2026-07-01), hyperswarm 4.17.2, hyperdht 6.35.1, sodium-native 5.2.0, udx-native 1.22.0. Node.js LTS on that date: v26.13.0.

Measurement note: the sizes marked "measured" were taken on 2026-11-03 by downloading the npm tarballs or nodejs.org artifacts and running `wc -c` on the extracted files. The URL given is the artifact that was measured.

---

## Q1. What is Bare, and how is it distributed (prebuilds, bare-runtime, libbare/bare.h, bare-make, bare-pack, bare-build)? Which platforms have prebuilds?

### Takeaway

Bare is Holepunch's small, modular JS runtime: V8 through libjs, plus libuv. It ships in three forms:

- **Prebuilt CLI executables:** the `bare-runtime-<platform>-<arch>` npm packages, available for win32-x64/arm64, darwin-arm64/x64, linux-x64/arm64, android and ios.
- **A C embedding library:** `libbare` with `include/bare.h`. You build it from source with `bare-make` (a CMake wrapper).
- **Tooling:** `bare-pack` makes `.bundle` files. `bare-build` makes standalone executables or app packages for every desktop OS.

All five desktop targets that matter here are Tier 1 with prebuilds.

### Cited Findings

- Bare is a "Small and modular JavaScript runtime for desktop and mobile". It is built on libjs ("bindings to V8 in an engine independent manner") and libuv. It provides a CJS+ESM module system, a native addon system (static and dynamic), and lightweight threads. Install it with `npm i -g bare`. — [holepunchto/bare README](https://github.com/holepunchto/bare)
- The build tool is `bare-make`: `npm i -g bare-make`, `npm i`, `bare-make generate`, `bare-make build`. It produces `bare(.exe)` in `build/bin`, plus `libbare.(a|lib)` and `(lib)bare.(dylib|dll|lib)`. Build options include `BARE_ENGINE` (default `github:holepunchto/libjs`), `BARE_ENGINE_SHARED` (default OFF), `BARE_PREBUILDS` (default ON) and `BARE_MEMORY_LIMIT`. — [holepunchto/bare README](https://github.com/holepunchto/bare)
- bare-make describes itself as an "Opinionated build system generator based on CMake" (v2.1.1, 2026-10-28). — [npm bare-make](https://www.npmjs.com/package/bare-make)
- Tier 1 platforms (prebuilds provided, tested in CI):
  - Linux arm64/x64 (kernel ≥ 5.15, glibc ≥ 2.35)
  - Android arm/arm64/ia32/x64 (≥ 10)
  - macOS arm64/x64 (≥ 13.0)
  - iOS arm64/x64 (≥ 15.0)
  - Windows arm64 (≥ 11) and x64 (≥ 10)

  Tier 2 covers other Linux architectures and musl. — [holepunchto/bare README](https://github.com/holepunchto/bare)

- The `bare-runtime` npm package is described as "Prebuilt Bare binaries for macOS, iOS, Linux, Android, and Windows". It declares optionalDependencies on these platform packages: `bare-runtime-{darwin-arm64, darwin-x64, linux-x64, linux-arm64, win32-x64, win32-arm64, android-*, ios-*}`. It exposes `bin: { bare: 'bin/bare' }`. — [npm bare-runtime](https://www.npmjs.com/package/bare-runtime)
- Contents of the platform packages (measured):
  - `bare-runtime-win32-x64@1.36.0` contains `bin/bare.exe` (PE32+ console, 46,233,088 B) and `lib/bare.lib` (1,405,418 B).
  - `bare-runtime-darwin-arm64@1.36.0` contains `bin/bare` (Mach-O arm64, 71,542,408 B, ad-hoc/linker-signed) and `lib/libbare.tbd` (4.1 MB).
  - `bare-runtime-linux-x64@1.36.0` contains `bin/bare` (97,094,392 B).

  Sources: [win32-x64 tarball](https://registry.npmjs.org/bare-runtime-win32-x64/-/bare-runtime-win32-x64-1.36.0.tgz), [darwin-arm64 tarball](https://registry.npmjs.org/bare-runtime-darwin-arm64/-/bare-runtime-darwin-arm64-1.36.0.tgz), [linux-x64 tarball](https://registry.npmjs.org/bare-runtime-linux-x64/-/bare-runtime-linux-x64-1.36.0.tgz)

- The macOS `bare` binary links only system libs (libSystem, CoreFoundation, libresolv, libc++). So it is self-contained, and V8 is statically linked (measured with `otool -L`). — [darwin-arm64 tarball](https://registry.npmjs.org/bare-runtime-darwin-arm64/-/bare-runtime-darwin-arm64-1.36.0.tgz)
- The GitHub releases of holepunchto/bare carry only a source tgz (`bare-1.36.2.tgz`, 8.8 KB). Binaries come through npm. — [GitHub releases API, holepunchto/bare](https://github.com/holepunchto/bare/releases)
- **bare-pack** "traverses a module graph and builds a `.bundle`", and by default embeds addons using prebuilds for the selected hosts.
  - Flags include `--host <platform>[-<arch>[-<environment>]]` (repeatable for multi-host bundles), `--linked` (addons resolve to `linked:` specifiers, "usually needed for mobile"), `--offload[-addons|-assets]`, `--builtins`, `--out` and `--format`.
  - Output formats: `bundle`, `bundle.cjs`, `bundle.mjs`, `bundle.json`.
  - Only the first matching host prebuild is bundled per addon.

  — [holepunchto/bare-pack README](https://github.com/holepunchto/bare-pack)

- **bare-build** (1.6.2) is an "Application builder for Bare". It packages JS as native app bundles or standalone executables.
  - `--standalone` output per platform: Windows gets a "PE executable with self-extracting `.dll` libraries", macOS a "Mach-O executable with self-extracting `.framework` libraries", Linux an "ELF executable with self-extracting `.so` libraries".
  - `--package` output: `.msix`, `.pkg` or `.AppImage`.
  - It ships "portable runtimes for all supported systems" that run only the libuv event loop, which suits headless use.
  - It embeds the bundle into a PE section `.bare`, a Mach-O segment `__BARE`, or ELF weak symbols.
  - It has signing flags: `--sign`, `--identity`, `--hardened-runtime`, `--entitlements`, `--subject`, `--thumbprint`, among others.
  - It accepts repeated `--host`.

  — [holepunchto/bare-build README](https://github.com/holepunchto/bare-build)

- Bare's own prebuild CI builds each OS on its native runner: windows-2022 for win32 x64/arm64, macos-15 for darwin, ubuntu for linux/android. It does not cross-compile Windows from macOS. — [bare prebuild.yml](https://github.com/holepunchto/bare/blob/main/.github/workflows/prebuild.yml)

### Inferences

- For a Tauri sidecar you can pull the prebuilt `bare.exe` from npm on macOS. Downloading an npm package for another platform works with `npm pack bare-runtime-win32-x64` or a direct tarball fetch. So the Windows build needs no compile step for Bare itself, and cross-building with cargo-xwin is unaffected.
- `bare-build --standalone --host win32-x64` may be able to produce a Windows standalone exe from macOS, because it ships prebuilt portable runtimes and accepts `--host`. The README does not say so explicitly, and I did not test it.
- `lib/bare.lib` (Windows) and `libbare.tbd` (macOS) in the runtime packages appear to be import or stub libraries, used to link native addons against the symbols exported by the `bare` executable. They do not appear to be static embedding libraries. Embedding libbare in-process therefore requires building from source.

### Gaps

- No official statement that bare-build/bare-pack cross-target Windows from a macOS host. I did not run them.
- No published size table for libbare as a static library, since none is published.

---

## Q2. Can libbare be embedded from Rust via FFI? Are there existing crates? What engine, what build requirements, and is cross-compiling for Windows MSVC from macOS feasible?

### Takeaway

Technically yes: the C API is small. But no maintained Rust crate wraps it, and building libbare/libjs requires CMake via bare-make plus static V8 prebuilds. Whole-archive linking and symbol export are mandatory because native addons resolve their symbols from the host. Cross-compiling libbare for MSVC from macOS is undocumented, and upstream CI does not do it. In-process embedding inside a cargo-xwin cross build is therefore high-risk.

### Cited Findings

- The C embedding API (`include/bare.h`), quoted verbatim:
  - `bare_setup(uv_default_loop(), platform, &env, argc, argv, options, &bare)`
  - `bare_load(bare, filename, source, &module)` (with `source == NULL` the file is read at runtime)
  - `bare_run(bare, UV_RUN_DEFAULT)`
  - `bare_teardown(bare, UV_RUN_DEFAULT, &exit_code)`
  - `bare_poll(bare, &timeout)`, a non-blocking loop step; timeout `-1` lets the host wait on `uv_backend_fd()`
  - `bare_attach` / `bare_detach`
  - `bare_suspend` / `bare_resume` / `bare_wakeup`
  - `bare_context_set` / `bare_context_get`

  — [holepunchto/bare README](https://github.com/holepunchto/bare)

- "Static `libbare` requires whole-archive linking, because Bare relies on constructor functions to register native addons." — [holepunchto/bare README](https://github.com/holepunchto/bare)
- The engine is libjs, "Simple and ABI stable C bindings to V8 built on libuv".
  - By default it builds against static V8 prebuilds from `holepunchto/chromium-prebuilds`.
  - A custom V8 needs a Chromium checkout plus `GN_DIR`/`GN_OUT_DIR`, bridged via `holepunchto/cmake-gn`.
  - Alternative ABI-compatible engines: libjsc (JavaScriptCore), libqjs (QuickJS), libmqjs, libjerry.

  — [holepunchto/libjs README](https://github.com/holepunchto/libjs)

- `bare-sidecar`'s CMake shows a reference embedding of libbare into a custom executable:
  - `fetch_package("github:holepunchto/bare@${bare_version}")`
  - `target_link_libraries(... $<LINK_LIBRARY:WHOLE_ARCHIVE,bare::static> ... bare::exports)`
  - `ENABLE_EXPORTS ON`
  - `link_bare_module(... bare-pipe)` and similar
  - `cmake_minimum_required(VERSION 4.0)`
  - on Windows, a `lib/bare.manifest` source

  — [bare-sidecar CMakeLists.txt](https://github.com/holepunchto/bare-sidecar/blob/main/CMakeLists.txt)

- Existing Rust FFI: the only public example found is `healthscience/cues-mantle`, `src/conduction/runtime.rs`. It hand-declares `extern "C"` bindings for `bare_setup`, `bare_load`, `bare_run`, `bare_teardown`, `uv_default_loop` and some `js_*` functions. It is a 0-star repo, last pushed 2026-05-19, so it is not a reusable crate. — [cues-mantle runtime.rs](https://github.com/healthscience/cues-mantle/blob/main/src/conduction/runtime.rs)
- crates.io has no `libbare`/`bare-sys` binding for Holepunch Bare. `libbare` 0.0.0 (2021) is an unrelated driver crate. — [crates.io search "libbare"](https://crates.io/search?q=libbare)
- There are pure-Rust ports of the Holepunch stack:
  - `peeroxide` 1.7.4: "Rust port of Hyperswarm"
  - `peeroxide-dht`: "Rust port of HyperDHT — Kademlia DHT with hole-punching and relay"
  - `libudx`: "Pure Rust implementation of the UDX protocol"
  - `hyperswarm` 0.1.0 and `hyperdht` 0.1.1 (2026-06-19)
  - `hypercore` 0.17.0-alpha.1

  All were updated in 2026. — [crates.io search "hyperswarm"](https://crates.io/search?q=hyperswarm), [crates.io search "hyperdht"](https://crates.io/search?q=hyperdht)

### Inferences

- **Feasibility of FFI.** A Rust crate could compile libbare via a `build.rs` that shells out to `bare-make` or `cmake`, link whole-archive (`+whole-archive` link modifier), and export symbols from the Tauri executable so dynamically loaded `.bare` addons can resolve `js_*`/`uv_*`/`bare_*`.
- **Why this is fragile.**
  - The Tauri binary would contain V8. That conflicts with WebView2 or WKWebView only in symbol space, not at runtime, so this is inferred safe but unverified.
  - The V8 static prebuild for x86_64-pc-windows-msvc would have to be fetched and linked under cargo-xwin, which upstream does not test.
- **Practical recommendation.** Avoid in-process libbare for the Windows cross-build path. Use a sidecar (Q4) or Bare Kit built natively per OS in CI.
- **Rust-native alternative.** The `peeroxide` family removes JS entirely. Whether it is wire-compatible with JS HyperDHT/blind-pairing is outside this note's scope; another researcher should verify.

### Gaps

- No documented clang/MSVC version requirements for libbare on Windows, and no docs on cross-compiling it.
- No information on whether V8 in the Tauri process conflicts with WebView2 on Windows.

---

## Q3. Does Bare Kit support desktop? What is a worklet and its IPC model?

### Takeaway

Bare Kit is a C library for embedding Bare in a host app. Each JS entry point runs as a "worklet" on its own thread inside the host process, with a non-blocking byte-stream IPC (`bare_ipc_*`) and `Bare.IPC` on the JS side. The README targets mobile, but CI builds and tests linux-x64/arm64, darwin-x64/arm64 and win32-x64/arm64. So desktop compiles and is tested, though not marketed. No binary prebuilds are released, so you build it with CMake per platform.

### Cited Findings

- "Bare Kit is a C library that embeds Bare in a host application… Each JavaScript entry point runs in its own thread, called a worklet, with an in-process IPC channel to the host." — [holepunchto/bare-kit README](https://github.com/holepunchto/bare-kit)
- C API names, verbatim:
  - Worklet: `bare_worklet_create`, `bare_worklet_options_t` (`version`, `memory_limit`), `bare_worklet_on_exit`, `bare_worklet_start`, `bare_worklet_terminate`, `bare_worklet_destroy`, `bare_worklet_suspend`, `bare_worklet_resume`, `bare_worklet_wakeup`
  - IPC: `bare_ipc_create`, `bare_ipc_read`, `bare_ipc_write`, `bare_ipc_close`, `bare_ipc_destroy`
  - Polling: `bare_ipc_poll_create`, `bare_ipc_poll_start`, `bare_ipc_poll_get_ipc`, `bare_ipc_readable`, `BARE_IPC_READ_BUFFER_SIZE`

  Reads and writes "never block and return `bare_ipc_would_block`". The JS side is the duplex stream `Bare.IPC`. — [holepunchto/bare-kit README](https://github.com/holepunchto/bare-kit)

- Linking notes:
  - Static executables need `ENABLE_EXPORTS`.
  - On Windows, static linking may need `WINDOWS_EXPORT_ALL_SYMBOLS`.
  - It mentions Electron/Node.js process-symbol conflicts.
  - Uncaught exceptions in a worklet "abort the whole host process by design" unless you register `uncaughtException`/`unhandledRejection` handlers.

  — [holepunchto/bare-kit README](https://github.com/holepunchto/bare-kit)

- The bare-kit CI `integrate.yml` test matrix covers linux x64 (ubuntu-22.04), linux arm64, android ×4, darwin x64 (macos-15-intel), darwin arm64 (macos-15), ios (device and simulator), win32 x64 (windows-2022, with `--sanitize address`) and win32 arm64 (windows-11-arm). It runs `bare-make test` on linux/darwin/win32. — [bare-kit integrate.yml](https://github.com/holepunchto/bare-kit/blob/main/.github/workflows/integrate.yml)
- The repo has `src/{apple, android, ios, unix, standalone, worklet.c, ipc.c, …}` and a `CMakeLists.txt`. The package is private (not published to npm), and release v3.0.1 has no binary assets. — [holepunchto/bare-kit](https://github.com/holepunchto/bare-kit), [npm 404 for bare-kit](https://www.npmjs.com/package/bare-kit)

### Inferences

- **Fit with Tauri.** Of all the in-process options, the Bare Kit API matches a Rust host best. It is a non-blocking read/write byte pipe that a tokio task can drive (poll callback → wake a channel).
- **Build cost.** It has the same build problem as libbare: CMake + V8 static libs, built natively per OS, then linked into the Tauri binary via `build.rs`. That conflicts with the current cargo-xwin-from-macOS Windows pipeline unless the Windows build moves to a Windows CI runner.
- **Stability risk.** The "abort the host on uncaught exception" behaviour is a reason to prefer an out-of-process sidecar: a P2P bug would otherwise crash the whole Tauri app.

### Gaps

- There is no official desktop integration guide for bare-kit, and no Rust bindings.

---

## Q4. Tauri 2 sidecar mechanics: externalBin, target-triple naming, shell plugin API, stdio, signing and NSIS

### Takeaway

Ship `bare(.exe)` (or a bare-build standalone exe) via `bundle.externalBin` with a `-<target-triple>` filename suffix, then spawn it with `app.shell().sidecar("name")`. Tauri's bundler signs sidecars automatically on macOS (inside-out, before the .app) and on Windows (if a signing config exists, skipping already-signed binaries). When cross-compiling from macOS, Windows signing needs a custom `signCommand`.

### Cited Findings

- **Configuration and naming**
  - `bundle.externalBin` lists paths relative to `src-tauri`.
  - Each file needs a `-$TARGET_TRIPLE` suffix, e.g. `binaries/my-sidecar-aarch64-apple-darwin` or `...-x86_64-unknown-linux-gnu`, with `.exe` on Windows.
  - Get the triple with `rustc --print host-tuple` (Rust ≥ 1.84).
  - The docs' Node rename script uses the host triple and "fails when cross-compiling".

  — [Tauri v2: Embedding External Binaries](https://v2.tauri.app/develop/sidecar/)

- **Rust API**
  - `use tauri_plugin_shell::ShellExt; let sidecar_command = app.shell().sidecar("my-sidecar").unwrap();` then `spawn()`.
  - Read `CommandEvent::Stdout(line_bytes)` from the receiver in an async task, and write stdin via `child.write(...)`.
  - `sidecar()` takes "just the filename, NOT the whole path".

  — [Tauri v2: Embedding External Binaries](https://v2.tauri.app/develop/sidecar/)

- **Permissions:** `shell:allow-execute` (for `execute()`) or `shell:allow-spawn` (for `spawn()`), with `"sidecar": true`. The `args` field accepts static strings, regex validators, `true` or `false`. These capabilities only matter when spawning from the JS side. — [Tauri v2: Embedding External Binaries](https://v2.tauri.app/develop/sidecar/)
- **Process lifecycle:** keep the child handle alive, because dropping it kills the child. — [Evil Martians: Rust + Tauri + sidecar](https://evilmartians.com/chronicles/making-desktop-apps-with-revved-up-potential-rust-tauri-sidecar). A reported issue has a Node sidecar still running after the app closed, so shutdown must be handled explicitly. — [tauri-apps/tauri#8689](https://github.com/tauri-apps/tauri/issues/8689)
- **macOS signing in the bundler**
  - `copy_binaries` results (external binaries) are added to `sign_paths` with `is_an_executable: true`.
  - The source comments: "Sign frameworks and sidecar binaries first, per apple, signing must be done inside out".

  — [tauri-bundler macos/app.rs](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle/macos/app.rs)

- **macOS notarization**
  - It is required with a Developer ID Application certificate.
  - Credentials come from `APPLE_SIGNING_IDENTITY`, `APPLE_CERTIFICATE`/`APPLE_CERTIFICATE_PASSWORD`, `APPLE_ID`/`APPLE_PASSWORD`/`APPLE_TEAM_ID`, or `APPLE_API_ISSUER`/`APPLE_API_KEY`/`APPLE_API_KEY_PATH`.
  - The page does not cover hardened-runtime entitlements.

  — [Tauri v2: macOS signing](https://v2.tauri.app/distribute/sign/macos/)

- **Windows signing in the bundler**
  - `sign_binaries_if_needed` iterates `settings.external_binaries()`.
  - If a sidecar is already signed it logs "sidecar … already signed. Skipping...". Otherwise it calls `windows::sign::try_sign`.
  - The NSIS bundler includes external binaries via `generate_binaries_data` and also signs NSIS plugin DLLs and the uninstaller.

  — [tauri-bundler bundle.rs](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle.rs), [tauri-bundler nsis/mod.rs](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle/windows/nsis/mod.rs)

- **Windows signing when cross-compiling:** "when you cross-compile from Linux or macOS, you must use a custom sign command as the default implementation only works on Windows machines". Documented tools are relic (Azure Key Vault) and `artifact-signing-cli` (Azure Artifact Signing). — [Tauri v2: Windows signing](https://v2.tauri.app/distribute/sign/windows/)
- **Ad-hoc signing:** the prebuilt macOS `bare` from npm is only ad-hoc/linker-signed (`codesign -dv`: `Signature=adhoc`), so Tauri must re-sign it with the Developer ID (measured). — [darwin-arm64 tarball](https://registry.npmjs.org/bare-runtime-darwin-arm64/-/bare-runtime-darwin-arm64-1.36.0.tgz)
- **Community precedent:** `tauri-plugin-tauribun` manages Bun sidecars (spawn, stdin messaging, kill). It is not Bare-specific, but it shows the pattern. — [docs.rs tauri-plugin-tauribun](https://docs.rs/crate/tauri-plugin-tauribun/0.1.0/source/src/desktop.rs)

### Inferences

- **cargo-xwin cross-build.** Place `binaries/bare-x86_64-pc-windows-msvc.exe` (from `bare-runtime-win32-x64` or `bare-sidecar`'s `prebuilds/win32-x64/bare.exe`) before running `tauri build --target x86_64-pc-windows-msvc`. Don't rely on the docs' host-triple rename script, which breaks under cross-compilation. Name files explicitly per target.
- **The JS bundle and addons** are separate from the binary. Ship the `.bundle` (from bare-pack, embedding the win32-x64 `.bare` addon prebuilds) as a Tauri `resources` entry, or use a bare-build `--standalone` exe that contains everything. Pass the resolved resource path as an argument to the sidecar.
- **Stdout framing.** The shell plugin's `CommandEvent::Stdout` delivers line-split chunks by default. For binary framed protocols, either use newline-delimited JSON, or spawn the sidecar path yourself with `tokio::process::Command` and resolve the binary path next to the main executable. This is inferred from the docs' "line_bytes" naming, and I did not verify whether a raw-output mode exists in tauri-plugin-shell 2.x.
- **V8 under hardened runtime.** V8 JIT normally needs `com.apple.security.cs.allow-jit` (and possibly `allow-unsigned-executable-memory`) on the sidecar's entitlements. This is not confirmed in Tauri or Bare docs. bare-build exposes `--hardened-runtime` and `--entitlements`, which suggests it is relevant.

### Gaps

- No Tauri doc explicitly covers sidecar entitlements for JIT engines. I found no Bare-specific notarization guidance.
- I did not verify whether tauri-plugin-shell has a raw (non-line-split) stdout mode in 2.x.

---

## Q5. Sizes: Bare vs Node SEA, bare-pack bundling, and native addon prebuilds

### Takeaway

A stripped Bare executable is about 42–44 MB per desktop platform (bare-sidecar prebuilds). The bare-runtime CLI binaries are 46 MB on Windows, 72 MB on macOS arm64 and 97 MB on Linux x64, and compress to about 17–27 MB gzip. Node 26 LTS binaries are 100 MB (node.exe) to 147 MB (macOS arm64) before adding the app. The P2P native addons are small: sodium-native about 0.8–1 MB and udx-native about 0.2–0.27 MB per platform. They ship as both `.bare` (Bare) and `.node` (Node) prebuilds for all target platforms.

### Cited Findings

- `bare-runtime` CLI binaries v1.36.0 (measured):
  - win32-x64 `bare.exe` 46.2 MB (gzip 17.4 MB; npm tgz 18.0 MB)
  - darwin-arm64 `bare` 71.5 MB (gzip 20.8 MB)
  - linux-x64 `bare` 97.1 MB (gzip 27.2 MB)

  npm `unpackedSize`: darwin-x64 77.7 MB, linux-arm64 98.4 MB, win32-arm64 45.2 MB. — [npm bare-runtime-win32-x64](https://www.npmjs.com/package/bare-runtime-win32-x64), [npm bare-runtime-darwin-arm64](https://www.npmjs.com/package/bare-runtime-darwin-arm64), [npm bare-runtime-linux-x64](https://www.npmjs.com/package/bare-runtime-linux-x64)

- `bare-sidecar@0.5.14` prebuilds (measured):
  - win32-x64 `bare.exe` 43.6 MB
  - win32-arm64 41.3 MB
  - darwin-arm64 42.0 MB
  - darwin-x64 43.4 MB
  - linux-x64 43.9 MB
  - linux-arm64 43.3 MB

  Its CMake uses `-dead_strip,-S,-x` (Apple) and `--gc-sections,--strip-all` (Linux) for Release. — [npm bare-sidecar tarball](https://registry.npmjs.org/bare-sidecar/-/bare-sidecar-0.5.14.tgz), [bare-sidecar CMakeLists.txt](https://github.com/holepunchto/bare-sidecar/blob/main/CMakeLists.txt)

- Node.js v26.13.0 LTS (measured):
  - `win-x64/node.exe` 100,097,864 B (about 100 MB)
  - darwin-arm64 `bin/node` 147,346,864 B
  - Download archives: darwin-arm64 tar.xz 30.6 MB, linux-x64 tar.xz 34.1 MB, win-x64 zip 41.2 MB

  — [nodejs.org/dist/v26.13.0](https://nodejs.org/dist/v26.13.0/)

- Node SEA:
  - Stability is "1.1 - Active development".
  - It is built with `node --build-sea sea-config.json` (v25.5+) or the older postject flow.
  - "Native addons (`.node` files) cannot be loaded directly from the VFS because `process.dlopen()` requires files on the real file system". The pattern is to write the addon to a temp file, then call `process.dlopen`.
  - Cross-platform SEAs need `useCodeCache`/`useSnapshot` false.
  - macOS needs `codesign --remove-signature` before injection and re-signing after. Windows uses `signtool`.

  — [Node.js SEA docs](https://nodejs.org/api/single-executable-applications.html)

- sodium-native 5.2.0 and udx-native 1.22.0 ship `prebuilds/<platform>-<arch>/<name>.bare` and `<name>.node` for android-_, darwin-arm64/x64, ios-_, linux-arm64/x64 (sodium also musl), and win32-arm64/x64. Both depend on `require-addon` and `which-runtime`/`bare-events`.

  | Addon         | Platform     | `.bare` | `.node` |
  | :------------ | :----------- | ------: | ------: |
  | sodium-native | win32-x64    |  815 KB |  708 KB |
  | sodium-native | darwin-arm64 |  786 KB |  680 KB |
  | sodium-native | linux-x64    |  987 KB |  894 KB |
  | udx-native    | win32-x64    |  266 KB |  240 KB |
  | udx-native    | darwin-arm64 |  225 KB |  196 KB |
  | udx-native    | linux-x64    |  208 KB |  215 KB |

  Measured. — [npm sodium-native](https://www.npmjs.com/package/sodium-native), [npm udx-native](https://www.npmjs.com/package/udx-native)

- bare-pack embeds addon prebuilds for the selected `--host` into the `.bundle` by default. `--offload-addons` writes them next to the output instead, and `--linked` is for ahead-of-time linked (mobile) builds. — [holepunchto/bare-pack README](https://github.com/holepunchto/bare-pack)
- bare-build `--standalone` produces a single exe with "self-extracting `.dll`/`.framework`/`.so` libraries", so addons are packaged automatically. — [holepunchto/bare-build README](https://github.com/holepunchto/bare-build)
- rocksdb-native 3.19.3 (2026-10-30) exists. It is only needed if Corestore/Hypercore storage is used; Hyperswarm, HyperDHT and secret-stream don't need it. — [npm rocksdb-native](https://www.npmjs.com/package/rocksdb-native)

### Inferences

- **Installer cost.** Adding a Bare sidecar costs about 42–46 MB uncompressed and about 17–20 MB compressed in the installer, plus about 1–2 MB of addons and JS. A Node SEA costs about 100 MB uncompressed on Windows (about 40 MB compressed), plus addon temp-extraction logic.
- **Recommended packaging.** Bare with a bare-pack bundle (addons embedded for one host) is the most compact and the officially intended path.
- **Hyperswarm dependencies.** A pure Hyperswarm/HyperDHT/blind-pairing graph needs only sodium-native and udx-native as native addons. This is inferred from hyperdht's dependency tree and was not fully enumerated here.

### Gaps

- I did not measure an actual bare-pack bundle size for hyperswarm + blind-pairing, or the size of a bare-build `--standalone` exe.
- I did not measure the compressed size of NSIS LZMA output.

---

## Q6. IPC bridge options between Rust and a Bare process or worker, and the pattern Pear/Keet use

### Takeaway

Holepunch's own pattern is a dedicated duplex byte stream ("IPC") between host and Bare. Inside Bare it appears as `Bare.IPC`:

- **Out-of-process (bare-sidecar, pear-runtime):** an extra pipe on fd 3.
- **In-process (Bare Kit):** the `bare_ipc_*` buffer.

On top of that stream they layer framed RPC with `bare-rpc`, which is librpc ABI-compatible. For a Rust/Tauri host the simplest robust choice is the child's stdin/stdout carrying length-prefixed frames (framed-stream style) or NDJSON, with bare-rpc framing optional. The fd-3 trick is easy on Unix but awkward from Rust on Windows.

### Cited Findings

- **bare-sidecar (0.5.14, pre-1.0)**
  - Purpose: "Start and manage Bare sidecar processes from Node.js and Electron. A sidecar is a separate Bare process … with a dedicated IPC channel exposed as a duplex stream on both ends". In the entry script: `Bare.IPC.on('data', (data) => Bare.IPC.write(data))`.
  - Host side: it spawns its bundled `bare` with `stdio: [...stdio, 'overlapped']` and uses `this._process.stdio[3]` as the IPC.
  - Child side: `lib/runtime.js` does `const ipc = new Pipe(3); Bare.IPC = ipc`, and calls `Bare.exit()` when that pipe closes, so the child dies when the host closes the pipe.

  — [bare-sidecar README](https://github.com/holepunchto/bare-sidecar), [bare-sidecar index.js](https://github.com/holepunchto/bare-sidecar/blob/main/index.js), [bare-sidecar lib/runtime.js](https://github.com/holepunchto/bare-sidecar/blob/main/lib/runtime.js)

- **pear-runtime (1.3.1, "MVP - EXPERIMENTAL")**
  - "Embeddable Runtime library for Pear with P2P OTA updates, Bare workers and storage APIs".
  - OS support: macOS, Linux, Windows.
  - `PearRuntime.run(path)` "Start[s] a bare worker. Returns a duplex stream, the IPC pipe."
  - Quick start is Electron (`hello-pear-electron`). The WebView variant is "Coming Soon".

  — [holepunchto/pear-runtime README](https://github.com/holepunchto/pear-runtime)

- **bare-rpc 1.4.0:** "librpc ABI compatible RPC for Bare". Usage is `new RPC(stream, onrequest)`, `rpc.request(cmd)`, `req.send(data)`, `await req.reply()`. Frames are length-prefixed with message type, ids, flags and a `maxFrameSize`. Invalid input destroys the stream with `RPCError` (`FRAME_TOO_LARGE`, `UNKNOWN_MESSAGE`, `INVALID_MESSAGE`). It links to `holepunchto/librpc` (a C implementation). — [holepunchto/bare-rpc README](https://github.com/holepunchto/bare-rpc)
- **bare-ipc 1.1.3:** "Lightweight pipe-based IPC for Bare", built on bare-pipe. `IPC.open()` returns `[portA, portB]` and `port.connect()` returns a duplex. Ports "can be transferred between Bare threads and processes". — [holepunchto/bare-ipc README](https://github.com/holepunchto/bare-ipc)
- **framed-stream 1.0.1:** "Read/write stream messages prefixed 8, 16, 24 or 32 bit length". — [npm framed-stream](https://www.npmjs.com/package/framed-stream)
- **Bare Kit (in-process):** `bare_ipc_read`/`bare_ipc_write` are non-blocking, with poll callbacks, and the JS side is `Bare.IPC`. — [holepunchto/bare-kit README](https://github.com/holepunchto/bare-kit)
- **Tauri side:** `child.write(bytes)` for stdin and `CommandEvent::Stdout` for stdout. — [Tauri v2 sidecar](https://v2.tauri.app/develop/sidecar/)

### Inferences

- **Recommended for Tauri.** Spawn the sidecar with piped stdin/stdout and run a length-prefixed framed protocol (framed-stream-compatible uint32 LE prefix, or bare-rpc frames) over it. Reserve stderr for logs, routed to `core::diagnostics`.
  - Rust side: `tokio_util::codec::LengthDelimitedCodec` over the child's stdout/stdin.
  - Bare side: use `bare-pipe` on fd 0/1, or `bare-process` stdin/stdout, piped into `framed-stream` and then bare-rpc or JSON.
  - Make sure nothing else writes to stdout (`console.log` in Bare goes to stdout), or move the protocol to a dedicated channel.
- **Replicating bare-sidecar's fd 3 from Rust**
  - Unix: `CommandExt::pre_exec` + `dup2`, or the `command-fds` crate.
  - Windows: Node passes extra stdio handles through the CRT `lpReserved2` STARTUPINFO block, which Rust's `std::process::Command` doesn't expose. Use stdio or a named pipe instead.
- **Alternatives**
  - Local TCP on 127.0.0.1 with a random port and auth token passed as argv. It is easy cross-platform but exposes a localhost port.
  - Unix domain sockets / Windows named pipes (`bare-pipe` supports named paths; Rust has `tokio::net::windows::named_pipe`). This is more code per OS.
- **Protomux is the wrong layer.** It multiplexes protocols over a single Hyperswarm/secret-stream connection (peer-to-peer). It is not a host↔runtime IPC, so it stays inside the Bare process.

### Gaps

- I did not verify that the plain `bare` CLI from bare-runtime (as opposed to bare-sidecar's custom runtime) sets up `Bare.IPC`. bare-sidecar needing its own `runtime.js` that assigns `Bare.IPC = new Pipe(3)` suggests the stock CLI does not.
- I found no documented Keet desktop internal IPC design. Keet is closed-source.

---

## Q7. Known projects combining Tauri or Electron with Bare/Hyperswarm

### Takeaway

Holepunch's own desktop path is Electron + Bare: pear-electron, pear-runtime with hello-pear-electron, and bare-sidecar "from Node.js and Electron". I found no public Tauri + Bare or Tauri + Hyperswarm project. The Rust-native route (peeroxide, hypercore crates) is the closest Rust-shell precedent.

### Cited Findings

- `pear-electron` 1.7.28 is the "Pear User-Interface Library for Electron" (last modified 2026-02-09). — [npm pear-electron](https://www.npmjs.com/package/pear-electron)
- pear-runtime's quick start is Electron (`git clone https://github.com/holepunchto/hello-pear-electron`), and its "WebView: Coming Soon". — [holepunchto/pear-runtime README](https://github.com/holepunchto/pear-runtime)
- bare-sidecar targets "Node.js and Electron" hosts. — [bare-sidecar README](https://github.com/holepunchto/bare-sidecar)
- A web search for Tauri + Hyperswarm + Bare returned no project combining them. Only generic sidecar guides came up (Node/Bun). — [Evil Martians sidecar article](https://evilmartians.com/chronicles/making-desktop-apps-with-revved-up-potential-rust-tauri-sidecar), [tauri-plugin-tauribun](https://docs.rs/crate/tauri-plugin-tauribun/0.1.0/source/src/desktop.rs)
- GitHub repository search for "tauri hyperswarm" and "tauri hypercore" returned 0 repositories (2026-11-03). — [GitHub search](https://github.com/search?q=tauri+hyperswarm&type=repositories)
- Rust ports: `peeroxide` / `peeroxide-dht` / `libudx` (1.7.4, 2026-11-03), and `hypercore` / `hypercore-protocol` (datrs lineage). — [crates.io peeroxide](https://crates.io/crates/peeroxide), [crates.io hypercore](https://crates.io/crates/hypercore)

### Inferences

- A Tauri + Bare-sidecar app would be novel but mechanically straightforward. It is the Tauri analogue of bare-sidecar/pear-runtime in Electron, with Rust replacing Node's `child_process`.

### Gaps

- I could not confirm Keet's exact desktop architecture from a primary source.

---

## Q8. Node.js fallback: can Hyperswarm/HyperDHT run on Node, and what are the trade-offs of a Node SEA sidecar?

### Takeaway

Yes. The core P2P addons ship `.node` prebuilds alongside `.bare` ones, so Hyperswarm, HyperDHT and secret-stream run on Node. A Node SEA sidecar costs about 2–3× the binary size (about 100–147 MB vs about 42–46 MB). It needs temp-file extraction for `.node` addons, and SEA is still "active development". In exchange you get the mature Node tooling and debugging story.

### Cited Findings

- sodium-native and udx-native include `prebuilds/<platform>/<name>.node` for darwin, linux and win32 x64/arm64 alongside `.bare` files, and use `require-addon`/`which-runtime` for runtime detection (measured from the tarballs). — [npm sodium-native](https://www.npmjs.com/package/sodium-native), [npm udx-native](https://www.npmjs.com/package/udx-native)
- bare-sidecar's package.json maps `child_process`, `fs`, `os`, `path` and `url` to `bare-*` equivalents under the `bare` condition and to Node builtins by `default`. This is the dual-runtime import-map pattern Holepunch modules use. — [bare-sidecar package.json](https://github.com/holepunchto/bare-sidecar/blob/main/package.json)
- `bare-node-runtime` 1.5.3 is a "Compatibility layer for Node.js builtins and globals in Bare". This is the reverse direction: Node code on Bare. — [npm bare-node-runtime](https://www.npmjs.com/package/bare-node-runtime)
- SEA constraints:
  - Stability 1.1.
  - Native addons must be extracted to disk before `process.dlopen`.
  - Cross-platform SEAs need `useCodeCache:false` and `useSnapshot:false`.
  - The blob's Node version must match the binary.
  - Re-signing is needed on macOS (and optionally on Windows).

  — [Node.js SEA docs](https://nodejs.org/api/single-executable-applications.html)

- Node v26.13.0 binary sizes: `node.exe` about 100 MB, darwin-arm64 `node` about 147 MB (measured). — [nodejs.org/dist/v26.13.0](https://nodejs.org/dist/v26.13.0/)
- `blind-pairing` 2.3.1 was last modified on npm on 2024-10-31, which is old relative to the rest of the stack. Check that it works with current hyperswarm 4.17 / hyperdht 6.35. — [npm blind-pairing](https://www.npmjs.com/package/blind-pairing)

### Inferences

- **Node SEA trade-offs**
  - Pros: familiar tooling, the Node inspector, and npm packages without a `bare` export condition work as-is.
  - Cons: larger size, addon temp-extraction (an antivirus/SmartScreen footprint on Windows), V8 snapshot/code-cache limitations when cross-building, and Node-on-desktop-only (no mobile code sharing with a Bare Kit phone app).
- **Code sharing with the phone.** The phone side would likely use Bare Kit, and Holepunch modules are dual-runtime. Choosing Bare on desktop lets the same JS (and bare-rpc schema) run on both ends.
- **Cross-build from macOS:** the Node SEA path needs the target-platform `node.exe` (downloadable) plus `node --build-sea` or postject. Docs only show a linux-x64-on-darwin example, so Windows-target SEA building from macOS is plausible but undocumented.

### Gaps

- I did not test hyperswarm + blind-pairing under Node 26, or under SEA specifically.
- The SEA docs give no official size overhead figure beyond the base binary.
