# Peer configuration value contracts

Native authority: WebRTC revision `ba469aa2093ba950066258ca0a59a6fbd1295582`, `api/peer_connection_interface.h` (`RTCConfiguration`, `IceServer`, `PortAllocatorConfig`), and the native `GetConfiguration`/`SetConfiguration` operations.

## Construction and compatibility

`RtcConfiguration` is an owned configuration value. `native_default()` and `Default` read the native safe constructor; `aggressive()` reads the native aggressive constructor. The safe preset does not force a data-channel m-line. `PeerConnectionOptions::default()` likewise reads native defaults, not a second table of Rust constants.

Existing exhaustive `PeerConfiguration` and `IceServer` literals continue to compile unchanged. Conversion preserves their fields and existing default behavior, including the always-negotiated data-channel m-line. Advanced callers use `create_peer_connection_with_configuration` locally or `ProductionSession::create_peer_with_configuration` with owned values. These do not require CXX or native pointers.

## Value inventory

| Native value group | Rust representation |
| --- | --- |
| ICE servers, candidate pool size, transport policy | `RtcConfiguration` fields; transport policy includes All, RelayOnly, NoHost and None |
| Single legacy URI, URL list, credentials, TLS policy, hostname, ALPN and elliptic curves | `RtcIceServer`; native URL precedence and parsing are retained |
| Certificates | `PeerConnectionOptions::certificates`, retained `Certificate` resources |
| Bundle, RTCP mux, SDP semantics | Typed policies, including deprecated native Plan B and Negotiate RTCP mux |
| IPv6/Wi-Fi/link-local and network preference | Native booleans/counts, optional adapter mask |
| TCP/candidate network policy | Typed enums |
| Audio jitter packet limit, acceleration and delay; screencast bitrate | Native integers/booleans and optional bitrate |
| Receiving timeout and backup ping interval | `Option<i32>`; None maps to the native undefined sentinel -1 |
| Other ICE checking, unwritable/inactive, stable ping, STUN keepalive and usage-report intervals | Native optional integer fields, without invented nonnegative restrictions |
| Gathering, candidate prioritization, TURN pruning, presumed relay writability, renomination, restart role, candidate surfacing | Typed policies and native booleans |
| Crypto, extmap mixed mode, implicit rollback, TURN logging ID | Owned crypto values, native booleans and owned string |
| VPN preference/list | Typed preference and owned IP/prefix masks |
| Port allocator flags/port range | `PortAllocatorOptions`; flags retain unknown native bits |
| Pacer burst | Optional `NativeTimeDelta`, including native positive/negative infinity |
| Any-address ports, always-negotiated data channels, SCTP stream count and SNAP | Native boolean/integer fields |
| Private-backed public configuration accessors | DSCP, environment-clock stats timestamps, CPU adaptation, suspend-below-minimum, prerender smoothing, experimental load estimator, and audio/video RTCP report intervals are typed fields |

`effective_turn_port_prune_policy()` preserves the native legacy boolean override: `prune_turn_ports` selects priority-based pruning, otherwise the explicit policy applies. Unsupported values and combinations are left to native validation. For example, packet-limit settings may be clamped inside NetEq and port limits may be narrowed downstream. Configuration readback reflects native configuration, not every downstream component's derived/clamped state. Ignore-TLS-certificate policy is expressible and insecure; its effects follow native TLS validation rather than an additional binding policy.

## Updates, identity and errors

`PeerConnection::configuration()` returns the native effective configuration plus the binding's additional TURN/TLS CA dependency. `set_configuration()` calls native `SetConfiguration` and preserves its typed error. Actor equivalents are `peer_configuration` and `set_peer_configuration`, taking a `SessionPeerId`. Local handles remain sequence-bound; actor callers can migrate owned configuration and retained certificate resources.

Native mutability rules apply. Bundle policy and certificate resource changes are rejected, even if a replacement certificate has equal PEM content. Cloning a certificate preserves native identity and permits unchanged-configuration submission. After a local description, changing candidate pool size returns InvalidModification. Failed native validation leaves effective configuration unchanged. The binding's extra trusted TURN/TLS CA is a creation-time dependency and cannot be replaced by SetConfiguration.

Candidate pool size exposes the native 0..=65535 range, not the legacy u8 ceiling. Creation checks this range before native allocator work because the pinned allocator only DCHECKs it before eagerly creating pooled sessions. Invalid values return InvalidRange, matching native update validation. Valid large pools can still be expensive. ICE validity and server parsing otherwise use native factory/update error paths.

Snapshots own strings, lists, crypto values and retained certificates. They contain no borrowed thread, port, factory, or peer pointer. ICE-server Debug redacts the password; it does not promise redaction of secrets embedded in URI strings or zeroization of owned credentials.

## Qualification and remaining work

`tests/peer_configuration.rs` exercises native presets, all value groups, full server snapshots, deprecated supported selectors, typed update/creation errors, unchanged state after rejection, certificate identity, and actor migration followed by an SDP fingerprint check. `tests/controlled_driver.rs` uses explicit certificates and nondefault ICE policies on both peers, verifies SDP fingerprints/renomination, rejects pool mutation after local-description application, and still exchanges SCTP application bytes through caller-pumped virtual networking with no added native threads. Cryptographic material and synchronous certificate generation remain outside deterministic replay, as documented in the certificate contracts.

This increment covers configuration values and native readback/update operations, not complete configuration parity. The native `turn_customizer` provider pointer and other caller-supplied creation/environment dependencies require safe provider contracts and separate behavioral qualification. Their absence is adapter work remaining, not a native limitation. Full application-facing parity remains open.
