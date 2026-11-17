# Holepunch/Hypercore stack: native Rust options and browser/webview access paths

Research date: 2026-11-03. Repo and crate dates come from the GitHub REST API (`gh api repos/...`) and the crates.io API (`https://crates.io/api/v1/crates/<name>`), queried on that date. Reference point: JS `hyperdht` is at **6.35.1**, published 2026-11-03 ([holepunchto/hyperdht](https://github.com/holepunchto/hyperdht), [npm hyperdht](https://www.npmjs.com/package/hyperdht)). JS `hyperswarm` is at 4.17.2 (commit 2026-09-22) ([holepunchto/hyperswarm](https://github.com/holepunchto/hyperswarm)). JS `@hyperswarm/secret-stream` is at 6.9.2 (2026-09-10) ([holepunchto/hyperswarm-secret-stream](https://github.com/holepunchto/hyperswarm-secret-stream)). JS `protomux` is at 3.13.0 (2026-10-12) ([holepunchto/protomux](https://github.com/holepunchto/protomux)).

## Q1: Are there Rust implementations of HyperDHT, Hyperswarm, UDX, secret-stream Noise, Protomux and Hypercore, and are they compatible with the current network?

### Takeaway

Yes. As of November 2026 there are two independent Rust ports of the full Hyperswarm stack (UDX, dht-rpc, HyperDHT, Noise/secret-stream and Hyperswarm), and both claim live interop with JS peers on the public network:

- **peeroxide** (Rightbracket): pure Rust, at v1.7.4 and released today. It is the most complete, but it is young (created April 2026) and mostly the work of one developer.
- **cowlicks/hyperswarm** (datrs ecosystem): `hyperdht` 0.1.1 and `hyperswarm` 0.1.0. It is marked "WIP / Unstable".

A third route is **hyperdht-cpp**, a C++ library with Rust FFI bindings. It covers HyperDHT only, is LGPL-3.0, and is wire-compatible with hyperdht 6.29.1. The datrs `hypercore` and `hypercore-protocol` crates still target Hypercore LTS v10, not v11.

### Cited Findings

**peeroxide (Rightbracket/peeroxide): the most complete pure-Rust port**

- **Claims:**
  - The repo describes itself as a "Rust implementation of the Hyperswarm P2P networking stack — wire-compatible with the existing Node.js network. Includes Kademlia DHT, Noise handshakes, UDP hole-punching, and BBR congestion control. Pure Rust, no C dependencies." — [GitHub repo](https://github.com/Rightbracket/peeroxide)
  - The README says it is "a faithful port targeting full interoperability ... Node.js peers can discover and connect to Rust peers and vice versa." — [README](https://github.com/Rightbracket/peeroxide)
- **Crate layering:**
  - `peeroxide` (Hyperswarm) → `peeroxide-dht` (Kademlia, Noise handshakes, hole-punching, relay) → `libudx` (pure-Rust UDX with BBR).
  - A `peeroxide-cli` crate adds lookup, announce, ping, cp, dd and chat commands. — [README](https://github.com/Rightbracket/peeroxide)
- **Interop testing:**
  - "All layers are validated via golden byte fixtures and live cross-language interop tests against Node.js reference implementations. ... pure Rust crypto (RustCrypto) ... Rust 2024 edition ... tokio." — [README](https://github.com/Rightbracket/peeroxide)
  - Status line: "Version 1.0.0 — initial public release. The full protocol stack is implemented and validated, including live network interoperability with the Node.js reference implementation." — [README](https://github.com/Rightbracket/peeroxide)
- **Public network:**
  - The library quick-start uses `SwarmConfig::with_public_bootstrap()`, which joins the public HyperDHT bootstrap network. — [README](https://github.com/Rightbracket/peeroxide)
  - Live tests against the public network exist (`live_bootstrap`, `live_announce_lookup`, ignored by default). Gate scripts cover compact-encoding, libudx, dht-rpc, hyperdht, encrypted connections, hole punching, relay and hyperswarm. Docker is used for NAT-simulation hole-punch tests. — [TESTING.md](https://github.com/Rightbracket/peeroxide/blob/main/TESTING.md)
- **Source modules:**
  - `peeroxide-dht/src` contains `holepuncher.rs`, `nat.rs`, `noise.rs`, `noise_wrap.rs`, `secret_stream.rs`, `secretstream.rs`, `protomux.rs`, `blind_relay.rs`, `relay_service.rs`, `socket_pool.rs`, `routing_table.rs` and `persistent.rs`.
  - So it includes secret-stream, Protomux and blind-relay ports, not just the DHT. — [peeroxide-dht/src](https://github.com/Rightbracket/peeroxide/tree/main/peeroxide-dht/src)
- **Releases and activity:**
  - crates.io: `peeroxide`, `peeroxide-dht` and `libudx` are at **1.7.4**, released 2026-11-03. The previous release was 1.7.3 on 2026-07-12. `libudx` was first published to crates.io 2026-04-25. — [crates.io libudx](https://crates.io/crates/libudx), [releases](https://github.com/Rightbracket/peeroxide/releases)
  - The repo was created 2026-04-25 and has 20 stars. Contributors: one human author (`eshork`, 32 commits), github-actions[bot] (9) and one other contributor (1 commit). — [GitHub API contributors](https://github.com/Rightbracket/peeroxide/graphs/contributors)
- **License:** MIT OR Apache-2.0, with THIRD_PARTY_NOTICES for upstream code. — [README](https://github.com/Rightbracket/peeroxide)
- **Platform coverage:**
  - CI jobs all run on `ubuntu-latest`; no Windows or macOS CI job was found in `ci.yml`. — [ci.yml](https://github.com/Rightbracket/peeroxide/blob/main/.github/workflows/ci.yml)
  - Prebuilt CLI binaries are published only for macOS and Linux (glibc x86_64 and aarch64), not Windows. — [README](https://github.com/Rightbracket/peeroxide)
- **No pinned JS version:** I did not find the README stating which exact JS hyperdht version it pins against.

**cowlicks/hyperswarm (crates `dht-rpc`, `hyperdht`, `hyperswarm`)**

- **Crates:**
  - `hyperdht` 0.1.1 and `hyperswarm` 0.1.0 were both published 2026-06-19. `dht-rpc` 0.0.3 was published 2026-08-24.
  - Download counts are small (hyperdht 104, hyperswarm 25). — [crates.io hyperdht](https://crates.io/crates/hyperdht), [crates.io hyperswarm](https://crates.io/crates/hyperswarm)
- **Status:** the README warns "⚠️ WARNING 🚧 Work in Progress ⚒️Unstable". Its roadmap is checked off for:
  - all dht-rpc query logic
  - `Dht.connect`
  - relayed connections
  - "server" functionality "allowing Rust to receive connections from JS hyperdht"
  - `swarm.join`
  - Its Kademlia code is "copied from libp2p-kad". — [README](https://github.com/cowlicks/hyperswarm)
- **IMPLEMENTATION_STATUS.md lists as done:**
  - bootstrap, lookup, find_peer, announce, unannounce and connect
  - `listen` (server)
  - "Direct peer-to-peer connections via Noise IK handshake"
  - "Relay connections (client → relay → server)"
  - "UDX stream transport"
  - It does **not** explicitly list UDP holepunching as complete in the section I read. — [IMPLEMENTATION_STATUS.md](https://github.com/cowlicks/hyperswarm/blob/main/IMPLEMENTATION_STATUS.md)
- **Dependencies:** the `hdht` crate depends on `udx` 0.2.0 (datrs/async-udx) and `hypercore_handshake` 0.6.0. — [hdht/Cargo.toml](https://github.com/cowlicks/hyperswarm/blob/main/hdht/Cargo.toml)
- **Activity:** last commit on main was 2026-08-24 ("chore: Release"); the repo was pushed 2026-10-29. 4 stars, MIT. — [GitHub](https://github.com/cowlicks/hyperswarm)

**Older or abandoned Rust efforts (do not use)**

- `Frando/hyperswarm-rs`: last push 2024-04-03. A search summary of it says "The Rust implementation does not find peers on a NodeJS bootstrap node." — [Frando/hyperswarm-rs](https://github.com/Frando/hyperswarm-rs)
- `datrs/hyperswarm-dht`: last push 2022-08-04. — [datrs/hyperswarm-dht](https://github.com/datrs/hyperswarm-dht)
- `cowlicks/hyperswarm-dht`: last push 2025-02-28, superseded by cowlicks/hyperswarm. — [cowlicks/hyperswarm-dht](https://github.com/cowlicks/hyperswarm-dht)

**UDX crates**

- `udx` 0.2.0 (datrs/async-udx), described as "Rust port of libudx". It was published 2025-12-10, last commit 2026-01-11, 456 downloads. — [crates.io udx](https://crates.io/crates/udx), [datrs/async-udx](https://github.com/datrs/async-udx)
- `libudx` 1.7.4 is peeroxide's "Pure Rust implementation of the UDX protocol (tokio-compatible)". This is **not** a binding to the C libudx, despite the name. — [crates.io libudx](https://crates.io/crates/libudx)
- No `udx-sys`, `hyperswarm-secret-stream`, `protomux` or `hyperdht-rs` crate exists on crates.io (API returned not-found). — [crates.io API](https://crates.io/api/v1/crates/udx-sys)

**datrs hypercore and hypercore-protocol**

- `hypercore` 0.17.0-alpha.1 was published 2026-08-07; the last stable was 0.16.0 (2026-02-18). 357 stars.
  - It is "a limited Rust port"; its goal is "binary compatibility with the LTS version with regards to disk storage".
  - Unchecked roadmap item: "Support the new manifest in the wire protocol to remain compatible with upcoming v11". — [datrs/hypercore](https://github.com/datrs/hypercore), [crates.io](https://crates.io/crates/hypercore)
- `hypercore-protocol` 0.7.2 was published 2026-08-03. "This crate targets the Hypercore LTS version 10."
  - It does the Noise handshake with `snow` plus RustCrypto `crypto_secretstream`, supports WASM, and has JS interop tests.
  - v11 manifest support is unchecked. — [datrs/hypercore-protocol-rs](https://github.com/datrs/hypercore-protocol-rs)

**Noise compatible with @hyperswarm/secret-stream**

- No standalone crate exists.
- The capability is embedded in:
  - peeroxide-dht (`noise.rs`, `secret_stream.rs`) — [peeroxide-dht/src](https://github.com/Rightbracket/peeroxide/tree/main/peeroxide-dht/src)
  - cowlicks' `hypercore_handshake` dependency plus "Noise IK handshake" — [IMPLEMENTATION_STATUS.md](https://github.com/cowlicks/hyperswarm/blob/main/IMPLEMENTATION_STATUS.md)
  - hypercore-protocol-rs (snow plus crypto_secretstream) — [README](https://github.com/datrs/hypercore-protocol-rs)

**hyperdht-cpp (C++ with Rust bindings)**

- It describes itself as "A C++ implementation of HyperDHT -- wire-compatible with the JavaScript reference".
- Features:
  - holepunching with 4 strategies: consistent, random, birthday paradox and blind relay
  - Noise IK (Ed25519) plus SecretStream (XChaCha20-Poly1305)
  - a 90-function C FFI
  - wrappers for Python, Rust (`hyperdht` and `hyperdht-sys` crates via bindgen) and Kotlin/JNI for Android
- Status: "Wire-compatible with JS hyperdht@6.29.1. Live-tested in both directions on the public network", with "behavioural parity ~93%".
- Platforms: Linux, macOS, Windows, Android and ESP32.
- Requires libuv **1.51.x**: "libuv 1.52.0/1.52.1 has a UDP regression that can stall established libudx connections".
- License: LGPL-3.0. Repo created 2026-04-19, last push 2026-10-29. — [jjacke13/hyperdht-cpp](https://github.com/jjacke13/hyperdht-cpp)
- It does HyperDHT only (no Hyperswarm topic layer is described in the README). — [README](https://github.com/jjacke13/hyperdht-cpp)

### Inferences

- **peeroxide** is the only option that is pure Rust, permissively licensed, covers the full stack (Hyperswarm, HyperDHT, holepunch, secret-stream, Protomux, UDX) and is actively released. It fits a Tauri backend best: no C toolchain and no JS runtime.
  - The risks are a single-maintainer bus factor, a 6-month track record, and no Windows CI. Windows is a stated target platform, so it would need its own testing.
  - Much of the repo appears to be built with AI coding agents (it has an AGENTS.md). That is not a defect in itself, but it argues for independent interop testing before relying on it.
- "Interop with hyperdht 6.x" is a moving target: JS hyperdht went from 6.29.1 (the version hyperdht-cpp validated against) to 6.35.1. The DHT wire format has been stable within 6.x, and both ports claim live interop now. Still, behavioural drift does happen, for example the hyperswarm 4.11 keep-alive and tie-break change that broke dht-relay (see Q3).
- If a Tauri desktop app uses peeroxide or cowlicks as the _server_, a phone running JS Hyperswarm (Bare or Pear mobile) should be able to connect by public key. Phone-to-desktop direct reachability still depends on NAT types.

### Gaps

- I could not confirm peeroxide's behaviour on Windows, or which exact JS hyperdht version its golden fixtures were generated from.
- I could not confirm whether cowlicks/hyperswarm implements UDP holepunching. Its status file lists direct and relayed connections but no explicit holepunch item in the portion I read.
- No independent third-party review or production user of either Rust port was found.

## Q2: Is the C libudx usable from Rust via bindgen, and does it contain the NAT holepunching logic?

### Takeaway

libudx (C) is only the transport: reliable, multiplexed, congestion-controlled streams over UDP. The holepunching and NAT logic lives in JS HyperDHT (`lib/holepuncher.js`, `lib/nat.js`), so binding libudx alone gives you no NAT traversal. No `udx-sys` crate exists. The Rust ports reimplement UDX natively instead of binding the C code. hyperdht-cpp is the one project that reimplemented HyperDHT's holepunch logic in C/C++ on top of libudx and exposes it to Rust via bindgen.

### Cited Findings

- **libudx (C):**
  - Description: "udx is reliable, multiplexed, and congestion-controlled streams over udp". Its single public header is `include/udx.h`.
  - It builds static and dynamic libraries "for use outside of Node.js" via bare-make.
  - Last commit was 2026-10-20 ("close socket in udx__get_link_mtu if connect() errors. (#331)"). Apache-2.0. — [holepunchto/libudx](https://github.com/holepunchto/libudx)
- **udx-native (the Node binding):** its API exposes `setTTL`, `send`/`trySend` with a TTL, a `firewall` callback and `stream.relayTo`. These are primitives HyperDHT uses, not a holepunch algorithm. Latest is 1.22.0 (2026-10-20). — [holepunchto/udx-native](https://github.com/holepunchto/udx-native)
- **HyperDHT JS** contains `holepuncher.js`, `nat.js`, `noise-wrap.js`, `secure-payload.js`, `socket-pool.js` and `relay-connection.js`. This confirms that holepunching logic lives above UDX. — [hyperdht/lib](https://github.com/holepunchto/hyperdht/tree/main/lib)
- **Rust bindings:** none on crates.io (`udx-sys` not found). The datrs `udx` crate is a Rust port, not FFI. — [crates.io udx](https://crates.io/crates/udx)
- **hyperdht-cpp:**
  - Builds on libudx and libuv and offers a Rust wrapper via bindgen (`hyperdht-sys` plus a safe `hyperdht` crate, documented in BUILDING-RUST.md).
  - It requires pinned libuv 1.51.0 and libsodium 1.0.22 submodules, and uses vcpkg on Windows.
  - Its Rust wrapper crate names collide with cowlicks' `hyperdht` on crates.io. — [hyperdht-cpp README](https://github.com/jjacke13/hyperdht-cpp)

### Inferences

- Binding C libudx from Rust is technically straightforward with bindgen (one C header plus libuv), but it is pointless on its own. You would still need to reimplement dht-rpc, HyperDHT handshake and holepunch, and secret-stream. peeroxide has already done that in pure Rust.
- I infer that hyperdht-cpp's Rust wrapper is not published to crates.io under the name `hyperdht`, because cowlicks owns that crate. Using it would mean a git dependency plus building libuv and libsodium on 3 OSes, plus LGPL dynamic-linking obligations.

### Gaps

- I did not verify whether hyperdht-cpp's Rust crates are published under different names, or how mature they are.

## Q3: Can a browser or webview join the swarm (dht-relay, hyperswarm-web, WebRTC), and what server does that require?

### Takeaway

A browser cannot speak HyperDHT/UDX directly. The official route, `@hyperswarm/dht-relay`, needs a Node (or other) host running real HyperDHT that the browser reaches over WebSocket. That host is a server and contradicts "serverless". The relay repo is now **archived on GitHub**, the README says "Do not use it in production", and open PRs report it is broken with hyperswarm ≥ 4.11. `hyperswarm-web` (WebRTC plus a WebSocket proxy) has been dead since 2022.

### Cited Findings

- **@hyperswarm/dht-relay status:**
  - The README says "This project is still experimental. **Do not use it in production**." Usage: `relay(new DHT(), stream)` on the relaying side, and `new DHT(stream)` on the relayed (browser) side.
  - It has TCP and WebSocket transports, and a `dht-relay` CLI server on port 49443 with optional TLS certificate and key. — [holepunchto/hyperswarm-dht-relay](https://github.com/holepunchto/hyperswarm-dht-relay)
  - The GitHub API reports `archived: true`. The last commit on main was 2025-11-01 ("Skip failing tests"). The repo was created 2021-11-02. — [GitHub](https://github.com/holepunchto/hyperswarm-dht-relay)
  - npm `@hyperswarm/dht-relay` latest is 0.4.3, with metadata last modified 2026-03-31. — [npm](https://www.npmjs.com/package/@hyperswarm/dht-relay)
- **The relay protocol needs a "custodial" mode:**
  - `handshake`, `connect` and `listen` messages carry a `custodial` flag. When set, the client sends its **64-byte secret key** to the relay.
  - The protocol is built on Protomux. — [README Protocol section](https://github.com/holepunchto/hyperswarm-dht-relay#protocol)
- **Non-custodial mode was broken:**
  - Open PR #28 (2026-07-12) says two bugs "made non-custodial mode silently unusable and led to the tests being skipped". Non-custodial servers "never announce".
  - Root cause: a compact-encoding 2.15.1 change from Nov 2024. — [PR #28](https://github.com/holepunchto/hyperswarm-dht-relay/pull/28)
- **Broken with current hyperswarm:**
  - Open PR #29 (2026-10-13): "Two defects ... break relayed peers on current hyperswarm."
  - Since hyperswarm 4.11, `existing.sendKeepAlive is not a function` errors throw. Destroyed pending streams leave "ghost" connections. — [PR #29](https://github.com/holepunchto/hyperswarm-dht-relay/pull/29)
- **Encryption on the browser-to-relay leg:**
  - The WebSocket wrapper "only adapts the interface of the WebSocket as the WebSocket API already provides its own framing and encryption". In other words, it relies on wss/TLS for encryption to the relay.
  - The TCP wrapper uses hyperswarm-secret-stream for framing and encryption. — [README](https://github.com/holepunchto/hyperswarm-dht-relay)
- **hyperswarm-web (RangerMauve):** "Implementation of the hyperswarm API for use in web browsers". Last commit was 2022-02-15 (v2.2.0), and npm was last modified 2022-05-05. — [RangerMauve/hyperswarm-web](https://github.com/RangerMauve/hyperswarm-web), [npm](https://www.npmjs.com/package/hyperswarm-web)
- **geut/hyperswarm-webrtc** ("Hyperswarm for WebRTC") was last pushed 2019-11-21. — [geut/hyperswarm-webrtc](https://github.com/geut/hyperswarm-webrtc)
- **Hybrid apps:** some current hobby apps pair Hyperswarm on desktop/CLI with WebRTC on the web. Example: termkeet, "Hyperswarm desktop/CLI + WebRTC web", pushed 2026-10-27. These appear to be bespoke bridges, not a protocol-level browser join. — [termkeet](https://github.com/Alex-88-foarfeca/termkeet)

### Inferences

- **Custodial mode:** the relay holds the user's Ed25519 secret key and performs the Noise handshake on the browser's behalf. The relay can then read the stream contents, so this is not end-to-end encrypted to the browser.
- **Non-custodial mode:** the browser keeps the key and signing is proxied. It was broken until at least PR #28, which is unmerged on an archived repo. This is inferred from the protocol fields and PRs; I did not trace the stream path in code.
- **Phone using a plain browser:** the only Holepunch-native path is a WebSocket relay hosted somewhere. The relay could be the desktop app itself, but then the phone needs a reachable address for the desktop, which defeats DHT holepunching.
  - The practical alternatives are a native or Bare-based phone app, or a non-Holepunch transport such as WebRTC with signalling.
  - WebRTC with signalling needs a signalling channel and often STUN/TURN. That is also server infrastructure, though the Hyperswarm connection itself could serve as signalling only if both ends speak Hyperswarm.
- Holepunch's own direction is native and mobile runtimes (Bare, Pear), not browsers. The archival of dht-relay is consistent with that.

### Gaps

- I could not find the archival date or an official deprecation notice for dht-relay. The last commit was 2025-11-01; the open PRs are from Jul and Oct 2026.
- I did not find an official Holepunch statement on browser support in docs.pears.com (not fetched, because of the tool budget).

## Q4: Can the Tauri webview run Hyperswarm JS directly, and are there Tauri plugins that expose UDP to JS?

### Takeaway

No. Hyperswarm JS needs raw UDP via `udx-native` (a native Node/Bare addon), and webviews such as WKWebView, WebView2 and WebKitGTK expose no UDP API. The browser Direct Sockets API (UDPSocket) is restricted to Chrome Isolated Web Apps, which Tauri webviews are not. Small Tauri UDP plugins exist, but driving the stack through them would mean porting udx-native's transport to JS-over-IPC, which is unrealistic. Running the stack in Rust (peeroxide) and exposing commands or events to Angular is the natural design.

### Cited Findings

- `udx-native` is a native addon ("udx is reliable, multiplexed, and congestion-controlled streams over udp"), used as `require('udx-native')`. — [holepunchto/udx-native](https://github.com/holepunchto/udx-native)
- **Direct Sockets API:**
  - Available only in Isolated Web Apps. It is gated by the `direct-sockets` permissions policy in the IWA manifest, and constructors reject with NotAllowedError without it.
  - It has "connected" and "bound" UDP modes. — [Chrome Developers: Direct Sockets](https://developer.chrome.com/docs/iwa/direct-sockets), [WICG explainer](https://github.com/WICG/direct-sockets/blob/main/docs/explainer.md)
- **Tauri UDP plugins:**
  - `tauri-plugin-udp` 0.1.2 ("UDP Socket for Tauri App") was last published 2025-02-18; the repo was last pushed 2025-02-18 and has 99 stars. — [crates.io](https://crates.io/crates/tauri-plugin-udp), [kuyoonjo/tauri-plugin-udp](https://github.com/kuyoonjo/tauri-plugin-udp)
  - The same author has `tauri-plugin-tcp` 0.1.2 (2025-07-21). `tauri-plugin-kcp` 0.1.2 (2026-09-19) exposes KCP sockets. — [crates.io search](https://crates.io/search?q=tauri%20socket)

### Inferences

- Even with a UDP plugin, the JS Hyperswarm stack expects the udx-native object API (streams, congestion control, `relayTo`, firewall hooks). There is no adapter, and per-packet IPC round-trips would be slow. The webview should stay a UI over Rust commands.
- Any webview-based use of Hyperswarm JS would need a sidecar JS runtime (Node or Bare), which is exactly what the native-Rust route avoids.

### Gaps

- I did not check whether WebView2 or WebKitGTK expose any experimental UDP or Direct Sockets flags. I found no evidence that they do.

## Q5: Are there implementations in other languages, and is the HyperDHT wire protocol specified anywhere?

### Takeaway

There is no formal spec. Upstream HyperDHT has only two prose and sequence-diagram docs (`docs/handshake.md`, `docs/holepunch.md`). Every independent implementation reverse-engineered the JS code, validated it with golden byte fixtures, and tracks the JS version by hand. Other ports: C++ (hyperdht-cpp, with Python, Kotlin and Rust wrappers), a 2021 Go stub, and a Python package that wraps the Node stack. No Zig port turned up.

### Cited Findings

- **Upstream protocol docs:** hyperdht's `docs/` folder contains only `handshake.md` and `holepunch.md`.
  - They describe the FIND_PEER, PEER_HANDSHAKE and PEER_HOLEPUNCH flows through client and server relay nodes, using FROM_CLIENT, FROM_RELAY, FROM_SERVER and REPLY modes, with tokens.
  - There are no byte-level message layouts. — [handshake.md](https://github.com/holepunchto/hyperdht/blob/main/docs/handshake.md), [holepunch.md](https://github.com/holepunchto/hyperdht/blob/main/docs/holepunch.md)
- **hyperdht-cpp** ships a "Wire protocol spec (PROTOCOL.md): Reverse-engineered from JS, 12 sections" and a JS-name mapping doc. It tracks parity gaps with JS file:line references in docs/TODO.md. — [hyperdht-cpp README](https://github.com/jjacke13/hyperdht-cpp)
- **peeroxide** validates against "golden byte fixtures" generated by Node scripts (`node tests/node/generate-golden.js`) and ships mdBook docs in `docs/`. — [TESTING.md](https://github.com/Rightbracket/peeroxide/blob/main/TESTING.md)
- **Go:** `tigerbot/hyperdht` was last pushed 2021-12-31 (2 stars). — [tigerbot/hyperdht](https://github.com/tigerbot/hyperdht)
- **Python:** KadePy "leverages the official, battle-tested Hyperswarm stack (Node.js) through a high-performance native binding". It was last pushed 2026-01-27, so it is a wrapper, not a reimplementation. — [ON00dev/KadePy](https://github.com/ON00dev/KadePy)
- **Ecosystem tooling:**
  - A Wireshark Lua dissector for HyperDHT (pushed 2026-08-07). — [K09-0/hyperdht-wireshark-dissector](https://github.com/K09-0/hyperdht-wireshark-dissector)
  - A public-network crawler and validator, hyperdht-observatory (2026-07-20). — [LamplitIsles/hyperdht-observatory](https://github.com/LamplitIsles/hyperdht-observatory)
  - A P2P VPN over HyperDHT built on hyperdht-cpp (nospoon, 2026-10-29). — [jjacke13/nospoon](https://github.com/jjacke13/nospoon)
- **Public bootstrap nodes** (as listed by hyperdht-cpp): `node1.hyperdht.org:49737`, `node2.hyperdht.org:49737`, `node3.hyperdht.org:49737`. — [hyperdht-cpp README](https://github.com/jjacke13/hyperdht-cpp)

### Inferences

- In 2026 several independent wire-compatible reimplementations appeared (peeroxide and hyperdht-cpp, both started April 2026, plus cowlicks' revived Rust work). That shows the protocol is implementable from source alone. But compatibility is defined by "whatever JS hyperdht does", so each upstream minor release carries drift risk.
- A Tauri app relying on a Rust port should pin and test against a specific JS hyperdht version on the phone side, and run its own interop CI.

### Gaps

- I did not read hyperdht-cpp's PROTOCOL.md or peeroxide's mdBook to judge their depth.
- I found no official Holepunch statement on supporting third-party implementations or on protocol versioning policy.
- I did not search Discord or HN discussions (tool budget).
