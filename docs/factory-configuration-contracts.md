# Factory configuration contracts

Native authority: WebRTC revision
`ba469aa2093ba950066258ca0a59a6fbd1295582`,
`api/peer_connection_interface.h`, `rtc_base/network_constants.h`,
`rtc_base/ssl_stream_adapter.h` and their native implementations. This inventory
covers factory `Options` and `SetOptions`, not the complete factory creation,
provider, media or peer-configuration interface.

| Native contract | Public Rust representation | Semantics |
| --- | --- | --- |
| `Options::disable_encryption` | `FactoryOptions::disable_encryption` | Defaults to false. Disables DTLS and native media encryption in new peers; prevents DTLS-dependent SCTP channels. |
| `Options::disable_network_monitor` | `FactoryOptions::disable_network_monitor` | Defaults to false. Forwarded unchanged, but the pinned Linux C++ factory stores this field without consulting it. Setting it does not disable an installed network monitor; behavior of other platform wrappers is not a Linux guarantee. |
| `Options::network_ignore_mask` | `FactoryOptions::ignored_network_adapters` | Defaults to loopback. `NetworkAdapterMask` preserves the open native bit pattern, including high bits. Known adapter categories have composable constants. |
| `Options::ssl_max_version` | `FactoryOptions::ssl_max_version` | Defaults to the native DTLS 1.3 alias. `SslProtocolVersion` preserves all native values, including signed unspecified and deprecated values. |
| `SetOptions` | `PeerConnectionFactory::set_options`, `ProductionSession::set_factory_options` | Replaces the complete options value for future peers. Existing peers retain their own options. The native method has no validation outcome and no getter. |
| Initial option application | `PeerConnectionFactoryBuilder::options` | Applied to the native factory before it is returned to the caller. The actor setter can be used before creating its first peer. |

`NetworkAdapterMask` covers Ethernet, Wi-Fi, generic cellular, VPN, loopback,
any-address and cellular 2G/3G/4G/5G categories. A zero mask ignores no adapter
category. Native bit masks are not interface identifiers or interface-name
filters. Unknown adapter types have native value zero and therefore no mask bit.

The native protocol selector is shared between TLS and DTLS and contains numeric
aliases. Rust exposes `Tls10`, `Tls11`, `Tls12`, `Tls13`, `Unspecified` and
`DTLS_10`/`DTLS_12`/`DTLS_13` aliases. For peer connections this field controls
DTLS, not TURN TLS. The pinned SSL implementation sets a DTLS 1.2 minimum;
unspecified and DTLS 1.3 allow 1.3 only if compiled in, and the other enum values
cap at 1.2. This does not enable obsolete DTLS versions or guarantee a negotiated
version. The native platform-network-monitor option is forwarded without a
binding policy override; configuration does not guarantee monitoring support
for a particular injected manager or platform.

## Ownership and execution

The value is copied on the factory signaling thread through native `SetOptions`.
It holds no Rust callback or borrowed provider. The local factory remains
sequence-bound and non-`Send`. `ProductionSession` requires mutable actor access,
retains its native role threads, and can configure the factory after caller
migration without exporting a local factory handle. A closed actor rejects
configuration with `PeerErrorKind::Closed`.

Controlled factories apply the same native options on the caller-owned thread.
They do not introduce a helper thread, native socket or wall-clock executor.
The network mask affects new native port allocators, including injected
networks, rather than being a Rust-side candidate-observation filter.

Public consumer examples in `tests/factory_configuration.rs` observe secure and
unencrypted native SDP across option updates, including actor migration, rather
than reading back a binding cache. `tests/controlled_driver.rs` observes host
candidate presence/absence after changing an injected Ethernet-network mask,
and actual caller-pumped SCTP exchange with a DTLS 1.2 maximum.
