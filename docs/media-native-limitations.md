# Pinned native media limitations

These limits apply to SDK `ba469aa2093ba950066258ca0a59a6fbd1295582`.
The binding does not patch libwebrtc or substitute another engine.

- **Three-rung reception:** the stock receiver does not accept simultaneous
  reception of all three simulcast rungs. If native negotiation collapses a
  sender to a single ambiguous encoding, direct three-rung input is declined
  rather than assigning a rung from geometry. This does not prevent independent
  L1T1/L1T3 senders on the same peer, or three-rung sending to a genuinely capable
  external answer. Native publication is never claimed to be atomic.
- **Two different same-format L1T3 FPS capabilities:** mixed L1T1/L1T3/
  three-rung source profiles use actual native initialization mode/stream
  fields and one installed factory. Two sources with identical H264 format and
  L1T3 mode but different FPS allocations cannot be causally distinguished by
  factory Create/GetEncoderInfo before input: those APIs receive no source or
  sender token. A native per-sender creation-context hook is required for this
  additional case. The binding does not substitute format tags, guess creation
  order or advertise a false shared FPS capability. The user explicitly accepted
  this documented boundary. Two independent L1T3 sources sharing one truthful
  temporal-fraction capability remain supported and exercised.
- **SFrame:** stock sender/receiver creation can enable negotiation, then returns
  the SDK's `UNSUPPORTED_OPERATION` because its encrypter/decrypter pipeline is
  unimplemented. Key interfaces do not supply that absent cipher pipeline.
  Use the owning caller frame-encryptor/decryptor interfaces for frame protection.
- **Installed video dependency rewrites:** the public transformer API does not
  expose the sender's current private dependency structure or decode-target
  cardinality. Arbitrary changes to frame IDs, dependency lists, spatial/temporal
  indices or decode-target indications cannot be certified at that consumer
  without an SDK hook. Installed setters reject those changes explicitly;
  ordinary native-generated dependency metadata is retained. Cross-route video
  forwarding into another sender callback is also rejected. Payload and other
  supported metadata transformations remain available subject to actual native
  consumer admission. No last-request snapshot, SSRC join or combined GFD/DD
  limit is used as a substitute for current native context.
- **Installed encoder adaptation:** current private scaling configuration and
  lookup resolution are not exposed at the encoder-info callback. Owning info
  values retain their full storage domain, but installation requires nonzero
  signed-representable alignment, including every supported simulcast rung,
  named pixel formats for native logging and valid reached bitrate-limit
  prefixes. Refusal records `CodecInfoFailed`; no metadata is silently clamped.
  The caller adapter has no fallback factory, so the stock forced-VP8 fallback
  subtraction is not reachable on that closed route.
- **Full encoded-output admission:** VP9 fixed-array/count and top-layer
  end-marker requirements are checked before installed packetization. Full
  callbacks must also fit the actual encoder initialization's stream count and
  codec. The SDK has no public getter for later private sender packetizer state;
  general independently retargeted output cannot be certified by a context-free
  callback. Use registration-specific initialized output capabilities, not
  stream-name/SSRC joins. Local owning callbacks preserve unrestricted metadata.
- **Simulcast short frame-type vectors:** the pinned split-adapter path has a
  native off-by-one read. Short vectors are refused before that consumer rather
  than patching the SDK or substituting absent/default types. Owning frame-type
  presence is preserved through safe native paths. Unsafe numeric frame-rate
  conversion and bandwidth multiplication are likewise refused at consumption,
  with persistent feedback-failure evidence for native void callbacks.
- **Synthetic transformable video construction:** the pinned public constructors
  abort rather than construct usable stock frames. Acquire actual SDK frames
  through installed transformation and use supported native cloning instead.
