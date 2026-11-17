# Pears/Holepunch P2P stack for phone-to-desktop: pairing, networking, licensing, project health (as of 2026-11-03)

Method note: Primary sources are GitHub READMEs, source code on `main` (fetched 2026-11-03), the npm registry, the GitHub API, and docs.pears.com. Web search turned up very little independent material (no audits, no published Holepunch success-rate figures). Where a claim comes from reading source code, the cited URL is the file on `main`. Line references may drift.

## 1. Pairing: blind-pairing, connect-by-key, firewall allow-listing, QR encoding, security properties

### Takeaway

There are two workable patterns. (a) The simplest is to put the desktop's 32-byte HyperDHT public key in the QR (z-base32, 52 chars) and have the phone call `dht.connect(key)` / `swarm.joinPeer(key)`. The desktop then pins the phone's key with the `firewall` callback. (b) `blind-pairing` gives a one-time invite (seed + discoveryKey + optional expiry + optional bootstrap nodes) that exchanges a secret (e.g. a room/Autobase key) over the DHT, but its default 7-minute poll interval and its Autobase-oriented design make it heavier than this use case needs. In both patterns the transport is Noise-authenticated against the dialed public key, so a MITM is not possible without the desktop's secret key.

### Cited Findings

**Connect by key**

- In HyperDHT, peers are identified by public key, not by IP. The key is looked up in the DHT to find an IP:port, so peers can connect "regardless of location, even when they change networks." — [Pear docs: Connect two peers by key](https://docs.pears.com/howto/connect-two-peers-by-key-with-hyperdht.html)
- Server side: `const server = node.createServer(); await server.listen(keyPair)`. Client side: `node.connect(publicKey)`. "The remote public key can be encoded as either a buffer, a hex string or a z-base32 string." — [hyperdht README](https://github.com/holepunchto/hyperdht/blob/main/README.md)
- `createServer({ firewall (remotePublicKey, remoteHandshakePayload) { ... } })` lets the server validate who may connect. "if you do return false, else return true" (i.e. return `true` to reject). `remoteHandshakePayload` "contains their ip and some more info". — [hyperdht README](https://github.com/holepunchto/hyperdht/blob/main/README.md)
- Hyperswarm has the same idea at the swarm level: `firewall: remotePublicKey => (true|false)`, where "If true, the connection will be rejected". `swarm.joinPeer(noisePublicKey)` connects directly to a known peer key and "will ensure that peer connections are reestablished in the event of failures." — [hyperswarm README](https://github.com/holepunchto/hyperswarm/blob/main/README.md)
- `DHT.keyPair([seed])` creates an ed25519 key pair and can be deterministic from a seed. The connection socket is a NoiseSecretStream; `socket.remotePublicKey` identifies the peer, and `socket.handshakeHash` is "a unique hash representing this crypto session (same on both sides)". — [hyperdht README](https://github.com/holepunchto/hyperdht/blob/main/README.md)
- Connections are "end to end encrypted Noise streams". — [hyperdht README](https://github.com/holepunchto/hyperdht/blob/main/README.md)

**QR payload encoding**

- `hypercore-id-encoding` `encode(key)` turns a 32-byte key into a z-base32 id. `decode()` accepts a 52-char z-base32 string, a 64-char hex string, or a "pear link". `normalize`/`isValid` are also provided. License: Apache-2.0. Last npm release: 1.3.0 (2024-03-26). — [hypercore-id-encoding README](https://github.com/holepunchto/hypercore-id-encoding); [npm registry](https://registry.npmjs.org/hypercore-id-encoding)
- Pear v3.0.0 (2026-07-14) switched `pear seed` to "use z32 encoding for all keys", so z32 is the house style. — [Pear v3.0.0 release](https://github.com/holepunchto/pear/releases/tag/v3.0.0)

**blind-pairing mechanics**

- `blind-pairing` is "Blind pairing over HyperDHT". API: `BlindPairing.createInvite(key)` returns `{ invite, publicKey, discoveryKey }`. The member calls `addMember({ discoveryKey, onadd(candidate) { candidate.open(publicKey); candidate.confirm({ key }) } })`. The candidate calls `addCandidate({ invite, userData, onadd(result) })` and awaits `c.pairing`. The README example pairs an Autobase key. — [blind-pairing README](https://github.com/holepunchto/blind-pairing)
- Flow from `blind-pairing-core`:
  1. The member creates an invitation (a new signing key pair) and shares `{ discoveryKey, seed }`, keeping `publicKey` aside.
  2. The candidate signs arbitrary `userData` with the invitation key pair and encrypts it to a key derived from the invite `publicKey`.
  3. The member decrypts and verifies the signature, which proves the candidate holds the secret key.
  4. The member confirms or denies, and the response may carry `{ key, encryptionKey }`.
  5. The candidate checks that `key` matches `discoveryKey`, which proves the remote peer has read access.

  — [blind-pairing-core README](https://github.com/holepunchto/blind-pairing-core)

- Invite wire format (compact-encoding): `flags`, `seed` (fixed32), optional `discoveryKey` (fixed32), optional `expires` (uint32, seconds), optional `additionalNodes` (bogon/private IPs filtered out), and a `testInvitation` flag. `createInvite(key, opts)` defaults: `expires = 0` (no expiry), `seed = randomBytes(32)`. An `INVITE_EXPIRED` error exists. — [blind-pairing-core index.js](https://github.com/holepunchto/blind-pairing-core/blob/main/index.js); [blind-pairing-core lib/messages.js](https://github.com/holepunchto/blind-pairing-core/blob/main/lib/messages.js)
- "Invites may contain additional nodes for you to use. These are known Peers allowing you to connect without DHT connectivity being required." The candidate decodes them with `decodeInvite` and adds them via `dht.addNode`. — [blind-pairing README](https://github.com/holepunchto/blind-pairing)
- Transport detail from source: the candidate stores its encrypted request with `dht.mutablePut` and announces an ephemeral key under a `pairingDiscoveryKey`. The member polls with `dht.lookup`. It also joins the swarm on the discoveryKey and can deliver over a direct Protomux channel. `DEFAULT_POLL = 7 * 60 * 1000` (7 minutes), and the README example overrides it to `poll: 5000`. — [blind-pairing index.js](https://github.com/holepunchto/blind-pairing/blob/main/index.js)
- There is a "DHT only exchange" test (commit 2025-08-04), so pairing can complete via DHT records without a holepunched connection. — [blind-pairing commits](https://github.com/holepunchto/blind-pairing/commits/main)
- Keet's room invites use this style of flow. A 2026-10 issue shows a Keet join stuck at "Waiting for someone to let you in. You will join the group automatically when there is someone online." — [hyperdht issue #306](https://github.com/holepunchto/hyperdht/issues/306)

### Inferences

- For a single desktop with a few phones, the simplest secure design is: QR = z32(desktop public key) + a short one-time secret, or just the key. The phone connects and sends its own public key plus the secret in the first message. The desktop adds the phone's key to a persisted allow-list and from then on rejects unknown keys in `firewall`. This needs only `hyperdht`/`hyperswarm`.
- The QR is the trust anchor: anyone who sees the QR can dial the desktop key. A one-time secret or blind-pairing's signed-proof flow, plus a confirmation prompt on the desktop, limits what a leaked QR exposes. Leaking the public key alone does not allow impersonating the desktop, because Noise authenticates the server's static key.
- If using blind-pairing for a QR flow, set `poll` low (seconds) and set `expires` explicitly. The defaults (7-minute poll, no expiry) would feel broken in a scan-and-go UX.
- Encoded size estimate: a z32 key is 52 chars. A blind-pairing invite (~1 + 32 + 32 + 4 bytes ≈ 69 bytes, more with `additionalNodes`) fits easily in a QR code.

### Gaps

- I found no formal written security analysis of blind-pairing, and no statement of whether invites are single-use by default.
- `blind-pairing` itself has had no npm release since 2.3.1 (2024-10-31). Its core module was released more recently (2.10.1, 2026-05-05). It is unclear whether the wrapper is considered stable or is now superseded by Autobase/Keet-internal flows.

## 2. NAT traversal: how holepunching works, NAT classes, success rates, CGNAT, IPv6

### Takeaway

HyperDHT classifies each side's NAT as OPEN / CONSISTENT / RANDOM and coordinates UDP holepunching through DHT nodes. It uses a birthday-paradox strategy (256 sockets on one side, about 1750 random-port probes over about 35 seconds on the other) when exactly one side is randomizing. It fails by design when both sides are randomizing, and then needs a relay. IPv6 is effectively unsupported: the 2018 "Support IPv6" issue is still open and the IPv6 PR was never merged. No Holepunch-published success rates were found.

### Cited Findings

- NAT classes in source: `FIREWALL = { UNKNOWN: 0, OPEN: 1, CONSISTENT: 2, RANDOM: 3 }`. — [hyperdht lib/constants.js](https://github.com/holepunchto/hyperdht/blob/main/lib/constants.js)
- Holepunch strategy in `holepuncher.js`:
  - consistent↔consistent → `_consistentProbe()`
  - local consistent + remote random → `_randomProbes` (`tries = 1750 // ~35s`, random ports 1000–65535)
  - local random + remote consistent → `_openBirthdaySockets` (`BIRTHDAY_SOCKETS = 256`) with low-TTL (5) punches, then `_keepAliveRandomNat`
  - both random → treated as "unstable"

  — [hyperdht lib/holepuncher.js](https://github.com/holepunchto/hyperdht/blob/main/lib/holepuncher.js)

- Error codes include `HOLEPUNCH_DOUBLE_RANDOMIZED_NATS`, `CANNOT_HOLEPUNCH`, `REMOTE_NOT_HOLEPUNCHABLE`, and `HOLEPUNCH_PROBE_TIMEOUT`. — [hyperdht lib/connect.js](https://github.com/holepunchto/hyperdht/blob/main/lib/connect.js)
- Random-NAT punching is rate-limited on the server: `_randomPunchLimit = 1 // set to one for extra safety for now` and `randomPunchInterval` defaults to 20000 ms ("min 20s between random punches"). If over the limit, the server answers `ERROR.TRY_LATER` (or relays if a relay token exists). — [hyperdht index.js](https://github.com/holepunchto/hyperdht/blob/main/index.js); [hyperdht lib/server.js](https://github.com/holepunchto/hyperdht/blob/main/lib/server.js)
- A LAN shortcut is raced against the holepunch ("Race the LAN shortcut with normal holepunch when possible"). It is enabled unless `localConnection: false`. — [hyperdht lib/connect.js](https://github.com/holepunchto/hyperdht/blob/main/lib/connect.js)
- The client can veto punching via the `opts.holepunch(remoteFirewall, localFirewall, remoteAddresses, localAddresses)` callback. — [hyperdht lib/connect.js](https://github.com/holepunchto/hyperdht/blob/main/lib/connect.js)
- NAT reporting from dht-rpc: "dht-rpc will automatically detect if you are firewalled". Also exposed: `node.firewalled`, a property indicating "if your node is likely behind a randomizing NAT", `nat-update` events, and `ephemeral` / adaptive mode ("might take ~20-30 mins for the node to turn persistent"). — [dht-rpc README](https://github.com/holepunchto/dht-rpc/blob/main/README.md)
- Official statement: "HyperDHT's holepunching will fail if both the client peer and the server peer are on randomizing NATs", in which case the connection "must be relayed through a third peer." — [Pear docs: Connect two peers by key](https://docs.pears.com/howto/connect-two-peers-by-key-with-hyperdht.html)
- Pear docs say randomizing NATs are "common on corporate and carrier-grade networks". — [Pear docs: Relay connections through a blind relay](https://docs.pears.com/how-to/connect-to-peers/relay-connections-through-a-blind-relay/)
- README claim: "You can run servers on normal home computers, as the DHT will UDP holepunch connections for you", and holepunching "make[s] sure connectivity works on most networks". No figure is given. — [hyperdht README](https://github.com/holepunchto/hyperdht/blob/main/README.md)
- IPv6:
  - hyperdht issue #1 "Support IPv6" (2018-10-08) is still open — [issue #1](https://github.com/holepunchto/hyperdht/issues/1)
  - PR #101 "Implement IPv6 support for peer connections" (2022) was closed without merging — [PR #101](https://github.com/holepunchto/hyperdht/pull/101)
  - dht-rpc issue #3 "IPv6 support" (2019) is still open — [dht-rpc #3](https://github.com/holepunchto/dht-rpc/issues/3)
  - The underlying UDX sockets do support IPv6 (binds to `::`, falls back to `0.0.0.0`) — [udx-native README](https://github.com/holepunchto/udx-native/blob/main/README.md)
- Generic baselines, not Hyperswarm-specific:
  - Ford et al. found about 82% of tested NATs support UDP hole punching — [bford.info p2pnat](https://bford.info/pub/net/p2pnat-abs/)
  - libp2p/IPFS measured about 72% success in early DCUtR data — [ProbeLab talk](https://probelab.io/talks/libp2p-nat-hole-punching-success-rate/)
  - A large-scale 2025 arXiv study covers decentralized NAT traversal — [arXiv 2510.27500](https://arxiv.org/pdf/2510.27500)
- A live 2026-10 report (Keet desktop on Windows 11 + iOS/Android Keet 4.23.1) describes a case where "Peer connectivity never completes". The reporter suspects NAT traversal or relay behavior. — [hyperdht issue #306](https://github.com/holepunchto/hyperdht/issues/306)

### Inferences

- Phone on mobile 4G/5G: many carriers use CGNAT, often with endpoint-dependent (randomizing) mapping. HyperDHT can still punch if the desktop's NAT is CONSISTENT (typical home router), via the birthday strategy, which can take up to about 35 seconds. It will fail if the desktop is also on a randomizing NAT (e.g. a corporate network). That is precisely the "phone on 4G + desktop at work" case, so a relay should be planned for.
- No IPv6 in HyperDHT means IPv6-only mobile networks (464XLAT/NAT64) rely on the carrier's IPv4 translation, which typically behaves like a randomizing NAT (inference, not verified).

### Gaps

- No Holepunch-published holepunch success rates, Mathias Buus talk figures, or Keet telemetry were found.
- No measurement specific to CGNAT and HyperDHT was found.

## 3. Relaying: relayThrough, blind-relay, Keet, relay host requirements

### Takeaway

HyperDHT does not relay by default. Relaying is opt-in via the `relayThrough` option (a relay public key, array, or function) on both `connect()` and `createServer()`, using the `blind-relay` protocol, which is TURN-like and forwards end-to-end-encrypted UDX streams. Pear ≥3.4.0 (2026-09-10) ships `pear blind-relay start` and `pear --relay <key>`. Keet uses other call participants as relays. The relay must be reachable by both peers. Docs suggest a VPS and do not literally require a public IP, but in practice it needs to be a CONSISTENT/OPEN host. Running one breaks the "no servers I operate" goal unless some other always-on peer you control does the relaying.

### Cited Findings

- "HyperDHT does not do any relaying by default." Keet is given as an example where other call participants act as relays, so more participants means stronger connectivity. — [Pear docs: Connect two peers by key](https://docs.pears.com/howto/connect-two-peers-by-key-with-hyperdht.html)
- blind-relay is a "Blind relay for UDX over Protomux channels... a host may accept pairing requests from other hosts and relay UDX stream messages between them, similar to Traversal Using Relays around NAT (TURN)". Protocol: both peers send `pair { isInitiator, token, id, seq }`, the relay matches on a pre-exchanged 32-byte token, and `unpair` releases. Stats are exposed on `server.stats`. License: Apache-2.0. Latest release 1.6.1 (2026-05-06). — [blind-relay README](https://github.com/holepunchto/blind-relay); [npm registry](https://registry.npmjs.org/blind-relay)
- In hyperdht source, `connect.js` does `const relayThrough = selectRelay(opts.relayThrough || null)` and generates `relay.token()`. `server.js` has `this.relayThrough = opts.relayThrough || null`. `selectRelay` accepts a key, an array (random pick), or a function. Other options: `relayKeepAlive` (default 5000 ms). — [hyperdht lib/connect.js](https://github.com/holepunchto/hyperdht/blob/main/lib/connect.js); [hyperdht lib/server.js](https://github.com/holepunchto/hyperdht/blob/main/lib/server.js)
- Relayed connections can upgrade to direct ones. The code keeps punching while relayed ("we are relayed, but the puncher is not done yet"), and open PR #317 records "the direct route when a relayed server connection upgrades". — [hyperdht lib/connect.js](https://github.com/holepunchto/hyperdht/blob/main/lib/connect.js); [PR #317](https://github.com/holepunchto/hyperdht/pull/317)
- Pear CLI v3.4.0 (2026-09-10) added `pear blind-relay start` ("run a blind relay for peers that cannot hole-punch") and `pear --relay <key>` ("relay connections through a peer"). — [Pear v3.4.0 release](https://github.com/holepunchto/pear/releases/tag/v3.4.0)
- Relay how-to:
  - Run the relay on "a VPS or any always-on host with working connectivity"; "a relay on one of the two peers' own networks defeats the purpose".
  - The relay key is stable across restarts.
  - Both peers must register the same relay key.
  - "The swarm connects directly when it can and uses the relay only when a connection needs relaying, or when the local DHT node is randomized."
  - A blind relay "stores nothing".

  — [Pear docs: Relay connections through a blind relay](https://docs.pears.com/how-to/connect-to-peers/relay-connections-through-a-blind-relay/)

- Relay work in progress (all open):
  - PR #255 "Feat/relay service mode" (2026-05) adds `dht.createRelayServer()`. It "does not add relay discovery or global relay advertisement. Peers still explicitly choose this relay with `relayThrough`" — [PR #255](https://github.com/holepunchto/hyperdht/pull/255)
  - PR #252 "Reuse relay connections across pairings" — [PR #252](https://github.com/holepunchto/hyperdht/pull/252)
  - Issue #303 asks to expose whether a stream is relayed — [issue #303](https://github.com/holepunchto/hyperdht/issues/303)
  - PR #318 "Fix client close of relayed connections" (2026-10-30) — [PR #318](https://github.com/holepunchto/hyperdht/pull/318)
- The DHT nodes themselves only relay signalling (handshake/holepunch messages), not data. `announce` can name "up to 3 nodes, indicating which DHT nodes can relay messages to you". — [hyperdht README](https://github.com/holepunchto/hyperdht/blob/main/README.md)

### Inferences

- There is no public, Holepunch-operated data relay for third-party apps. Nothing in the docs suggests one, and PR #255 explicitly has no relay discovery. A truly serverless design therefore has no fallback when both sides are randomizing.
- Options for this app:
  - accept failures in double-randomized cases
  - run a cheap VPS relay, which conflicts with "serverless"
  - use a peer you already control with a good NAT (e.g. a home machine) as the relay
- A relay behind a CONSISTENT NAT (not a public IP) can probably work, since peers holepunch to it. A relay on an OPEN/public IP is the safe choice. The docs do not say this explicitly.
- Relay APIs are actively changing (multiple open PRs in Q4 2026), so the API surface is not settled.

### Gaps

- There is no README-level documentation of `relayThrough` in the hyperdht README. It is documented only via code and the Pear CLI how-to.
- Keet's exact relay policy beyond "other participants act as relays" (e.g. whether Holepunch runs relays for Keet) was not found.

## 4. Bootstrap: default nodes, private DHT, testnets

### Takeaway

By default every node bootstraps from three hard-coded nodes (`node{1,2,3}.hyperdht.org:49737`), with IPs pinned so DNS is not required. A fully private DHT is supported (`DHT.bootstrapper`, `hyperdht --bootstrap`), but it needs at least one persistent, publicly reachable node, i.e. a server. `hyperdht/testnet` exists for tests.

### Cited Findings

- Defaults: `['88.99.3.86@node1.hyperdht.org:49737', '142.93.90.113@node2.hyperdht.org:49737', '138.68.147.8@node3.hyperdht.org:49737']`. Format: "Supports suggested-IP to avoid DNS calls: [suggested-IP@]<host>:<port>". In a Pear app the default is `Pear.config.dht.bootstrap`. — [hyperdht README](https://github.com/holepunchto/hyperdht/blob/main/README.md); [hyperdht lib/constants.js](https://github.com/holepunchto/hyperdht/blob/main/lib/constants.js)
- "The default bootstrap servers are publicly served on behalf of the commons." No named operator is given. — [hyperdht README](https://github.com/holepunchto/hyperdht/blob/main/README.md)
- Hosting, from whois run on 2026-11-03 (no public URL; primary data): 88.99.3.86 = Hetzner Online GmbH (fsn1-dc8); 142.93.90.113 and 138.68.147.8 = DigitalOcean, LLC. `dig` confirms node1–3.hyperdht.org resolve to those IPs.
- Private network: "start one or more dht nodes with an empty bootstrap array... You'll need at least one persistent node". CLI: `hyperdht --bootstrap --host (server-ip)`, default port 49737, "it requires the port to be open". You "need to wait ~30 mins for the node to become persistent". — [hyperdht README](https://github.com/holepunchto/hyperdht/blob/main/README.md)
- Testnet: `import createTestnet from 'hyperdht/testnet.js'`, then `new Hyperswarm({ bootstrap: t.bootstrap })`. — [blind-pairing README](https://github.com/holepunchto/blind-pairing)

### Inferences

- Using the default bootstrap nodes means depending on infrastructure that is presumably operated by Holepunch. The operator is not explicitly stated, and there is no SLA. Bootstrap is needed only to join the DHT; once joined, the routing table spans many peers. A device that has been offline must reach a bootstrap node or a cached peer to rejoin.
- blind-pairing's `additionalNodes` and hyperdht's `connect({ nodes })` can carry known node addresses in the QR or persisted state, which reduces dependence on bootstrap hostnames.

### Gaps

- Holepunch does not publish uptime or operator commitments for the bootstrap nodes.
- Size of the public DHT is not published.

## 5. Firewalls, UDP-only transport, corporate networks, DNS filtering (Cisco Umbrella)

### Takeaway

The stack is UDP-only (UDX). There is no TCP or HTTPS/WebSocket fallback in hyperdht/hyperswarm, so a network that blocks outbound UDP blocks the stack entirely. A relay doesn't help either, because the relay connection is also UDX over UDP. DNS-based blocking of `*.hyperdht.org` (e.g. Umbrella) is partly mitigated because bootstrap entries carry hard-coded IPs. IP- or protocol-level blocking is not mitigated.

### Cited Findings

- hyperdht switched its transport to `udx-native` in 2022 (PR #82). UDX is "reliable, multiplexed, and congestion-controlled streams over UDP". — [hyperdht PR #82](https://github.com/holepunchto/hyperdht/pull/82); [udx-native README](https://github.com/holepunchto/udx-native/blob/main/README.md)
- blind-relay relays "UDX stream messages", so the relay path is UDP as well. — [blind-relay README](https://github.com/holepunchto/blind-relay)
- Hyperswarm issue #1 "Pluggable transports" (2018, Tor/I2P/libp2p) is still open, meaning there is no transport abstraction for TCP/WebSocket fallback. — [hyperswarm issue #1](https://github.com/holepunchto/hyperswarm/issues/1)
- Bootstrap port is UDP 49737. Client sockets use ephemeral ports; random-NAT probing targets ports 1000–65535. — [hyperdht README](https://github.com/holepunchto/hyperdht/blob/main/README.md); [hyperdht lib/holepuncher.js](https://github.com/holepunchto/hyperdht/blob/main/lib/holepuncher.js)
- The suggested-IP syntax exists "to avoid DNS calls". — [hyperdht README](https://github.com/holepunchto/hyperdht/blob/main/README.md)
- Randomizing NATs are "common on corporate and carrier-grade networks". — [Pear docs: blind relay](https://docs.pears.com/how-to/connect-to-peers/relay-connections-through-a-blind-relay/)
- General: "many network managers block all UDP traffic, with the exception of a few critical ports (such as DNS)". — [O'Reilly, Internet Core Protocols](https://www.oreilly.com/library/view/internet-core-protocols/1565925726/ch06s03s04.html)

### Inferences

- With Cisco Umbrella:
  - DNS-layer blocking of `hyperdht.org` alone likely does not break bootstrap, because the IPs are embedded.
  - If the corporate firewall or Umbrella SIG blocks arbitrary outbound UDP, or the "P2P" application category, the desktop cannot join the DHT at all.
  - If outbound UDP is allowed but the corporate NAT is randomizing, phone-on-4G ↔ desktop-at-work will need a relay that sits outside the corporate network.

  This should be tested empirically, e.g. by running `hyperdht` and checking `dht.firewalled` / NAT type and the connect error codes.

- Payloads are Noise-encrypted with no TLS/SNI signature, which may be flagged as unknown UDP by DPI firewalls (speculative).

### Gaps

- No documentation or issue was found about Cisco Umbrella specifically, or about enterprise deployment guidance from Holepunch.
- No official list of required egress ports was found.

## 6. Licensing

### Takeaway

All relevant modules are permissive. Networking core (hyperdht, hyperswarm, dht-rpc, hypercore) is MIT. Bare, bare-kit, pear, udx-native, blind-pairing(-core), blind-relay, and hypercore-id-encoding are Apache-2.0. No CLA or commercial-licence constraints were found.

### Cited Findings

- npm registry license fields on 2026-11-03:
  - MIT: hyperdht 6.35.1, hyperswarm 4.17.2, dht-rpc 6.28.0, hypercore 11.40.0
  - Apache-2.0: blind-pairing 2.3.1, blind-pairing-core 2.10.1, blind-relay 1.6.1, hypercore-id-encoding 1.3.0, udx-native 1.22.0, pear 3.0.0

  — [npm hyperdht](https://registry.npmjs.org/hyperdht); [npm hyperswarm](https://registry.npmjs.org/hyperswarm); [npm blind-pairing](https://registry.npmjs.org/blind-pairing); [npm blind-relay](https://registry.npmjs.org/blind-relay)

- GitHub license detection: bare = Apache-2.0, bare-kit = Apache-2.0, pear = Apache-2.0, hyperdht = MIT, hyperswarm = MIT. — [holepunchto/bare](https://github.com/holepunchto/bare); [holepunchto/bare-kit](https://github.com/holepunchto/bare-kit); [holepunchto/hyperdht](https://github.com/holepunchto/hyperdht)

### Inferences

- MIT and Apache-2.0 are compatible with closed or commercial distribution. Apache-2.0 adds NOTICE and patent-grant terms. Note that the `pear` npm package name (3.0.0) is only the CLI bootstrap; the runtime itself is delivered P2P.

### Gaps

- No CLA requirement was found in the repos checked, but contribution guidelines were not exhaustively reviewed.
- No SECURITY.md exists in hyperdht, hyperswarm, or bare (GitHub API returned 404).

## 7. Maintenance, backing, production use, breaking changes, audits

### Takeaway

The stack is very actively maintained as of 2026-11:

- hyperdht had a release today (6.35.1, 2026-11-03), 51 commits in 2026, and open relay PRs.
- Pear CLI ships every ~2 weeks (v3.0.0 on 2026-07-14 through v3.6.0 on 2026-10-15).

It is backed by Tether/Bitfinex and dogfooded by Keet. Risks:

- small contributor base (27 hyperdht contributors)
- frequent breaking changes at the Pear layer
- no public security audit
- small-star satellite modules (blind-pairing: 9 stars, no release since 2024-10)

### Cited Findings

**GitHub and npm activity (2026-11-03)**

| Repo               | Stars | Forks | Open issues | Last push  | Latest release      |
| ------------------ | ----- | ----- | ----------- | ---------- | ------------------- |
| hyperdht           | 417   | 83    | 52          | 2026-11-03 | 6.35.1 (2026-11-03) |
| hyperswarm         | 1344  | 118   | 52          | 2026-09-22 | 4.17.2 (2026-09-22) |
| bare               | 1249  | 38    | 1           | 2026-10-30 | –                   |
| bare-kit           | 46    | 12    | 0           | 2026-10-30 | –                   |
| pear               | 439   | 36    | 5           | 2026-11-03 | –                   |
| blind-pairing      | 9     | 2     | 0           | 2025-10-03 | 2.3.1 (2024-10-31)  |
| blind-pairing-core | 9     | 0     | 0           | 2026-05-05 | –                   |
| blind-relay        | 9     | 3     | 9           | 2026-09-08 | –                   |
| udx-native         | 29    | 15    | 9           | 2026-11-02 | 1.22.0 (2026-10-20) |

- Sources: [hyperdht](https://github.com/holepunchto/hyperdht), [hyperswarm](https://github.com/holepunchto/hyperswarm), [bare](https://github.com/holepunchto/bare), [pear](https://github.com/holepunchto/pear), [blind-pairing](https://github.com/holepunchto/blind-pairing), [npm registry](https://registry.npmjs.org/hyperdht).
- hyperdht had 51 commits since 2026-01-01 and 27 contributors. The holepunchto org has 777 public repos. — [GitHub API: holepunchto](https://github.com/holepunchto)
- There is no hyperdht-relay repo; `holepunchto/hyperdht-relay` returns 404. Relaying lives in `blind-relay` plus hyperdht's `relayThrough`. — [GitHub](https://github.com/holepunchto/blind-relay)

**Pear releases and breaking changes**

- Releases: v3.0.0 2026-07-14, v3.1.0 07-30, v3.2.0 08-13, v3.3.0 08-27, v3.4.0 09-10, v3.5.0 09-30, v3.6.0 10-15. — [Pear releases](https://github.com/holepunchto/pear/releases)
- v3.0.0 breaking changes: "MAJOR - `pear run` has been removed", `pear://<alias>` removed, `pear sidecar --key` removed, and the "New pear-runtime architecture - platform reworked onto the new modular pear-runtime structure". — [Pear v3.0.0](https://github.com/holepunchto/pear/releases/tag/v3.0.0)
- The Pear v2.0.0 changelog adds sidecar GC, `pear data`, new `pear-api` constructors, etc. v2.x continued to v2.6.5. — [Pear CHANGELOG](https://github.com/holepunchto/pear/blob/main/CHANGELOG.md)

**Backing**

- Holepunch was launched in July 2022 by Tether, Bitfinex and Hypercore, with Keet as the first app. — [CoinDesk 2022-07-25](https://www.coindesk.com/tech/2022/07/25/tether-bitfinex-hypercore-launch-encrypted-communications-protocol-holepunch/)
- Tether and Bitfinex had invested $10M, with up to $100M more planned. — [The Block](https://www.theblock.co/post/159423/bitfinex-tether-video-calling-app-keet-holepunch-hypercore)
- Pear Runtime was launched as open source in February 2024, and Holepunch is described as "a Tether-backed company". — [Pears.com news](https://pears.com/news/holepunch-unveils-groundbreaking-open-source-peer-to-peer-app-development-platform-pear-runtime/); [SD Times](https://sdtimes.com/open-source/holepunch-launches-pear-runtime-an-open-source-peer-to-peer-app-development-platform/)

**Production use**

- Keet runs in production on desktop and mobile (Keet Desktop 423.0, iOS/Android 4.23.1 as of 2026-10). — [hyperdht issue #306](https://github.com/holepunchto/hyperdht/issues/306)

**Security audits**

- No public security audit of hyperdht, hyperswarm, blind-pairing, or Keet was found in searches. A search for "audit" in the holepunchto org returns only `npm audit` / `hypercore-audit` (data-integrity) items. — [GitHub search](https://github.com/search?q=org%3Aholepunchto+audit&type=issues)

### Inferences

- The low-level networking modules (hyperdht/hyperswarm/udx) are the most stable layer: same major version for years, with frequent patch and minor releases. Pear CLI/runtime is the most volatile layer (two major versions in about a year). For a Tauri app, depending only on hyperdht/hyperswarm (via Bare/bare-kit on mobile, or Node) avoids Pear churn.
- Small-star satellite modules (blind-pairing, blind-relay) are maintained mostly by Holepunch staff for Keet's needs. Expect undocumented behavior and API drift.

### Gaps

- No Keet user numbers or 2025–2026 funding updates were found (search returned only 2022 coverage).
- No reliable date for Pear v2.0.0 was found.
- No public community-size metric (Discord membership etc.) was found.
- No third-party security audit was found. Absence of evidence is not proof that none exists.
