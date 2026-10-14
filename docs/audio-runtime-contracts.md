# Audio runtime contracts

## Caller codec installation

Caller audio factory, encoder, decoder and parsed-frame handles are standalone
owners. They are not selectable peer or `ProductionSession` codec dependencies.
The pinned SDK does not provide the recoverable error paths required to install
arbitrary fallible caller encoders safely:

- The voice engine repeats `QueryAudioEncoder` and consumes an optional result
  after only a DCHECK. A caller that previously reported support can subsequently
  reject the same format, return an error, panic or reenter. Returning nullopt
  cannot safely reject that later native operation.
- `AudioEncoder::Encode` returns `EncodedInfo`, not an error. Native sample and
  appended-byte invariants use fatal checks. ACM treats empty output as buffering
  or an empty packet, so empty metadata is not an encoding-failure channel.

Standalone wrappers preserve current Query answers and typed callback failures.
They do not authorize installed use, cache a prior success, fabricate supported
metadata, substitute stock products, or turn encoding errors into successful
buffering. Installed fallible caller encoders require a genuine native consumer
error path, unavailable under the current SDK pin and no-runtime-patch policy.

## Standalone NetEq decoder selection

`NetEq::stock_with_decoder_factory` and
`NetEqFactory::create_with_decoder_factory` preserve the supplied optional
factory. `None` means a genuine absent SDK factory, not a builtin default.
The original `stock` and `create` methods select the explicitly supplied factory.
Creation contexts retain the actual environment and optional factory independently
of the registration and outer owners.

Stock registration remains lazy even with no decoder factory. Ordinary codec
creation and packet consumption are refused before the pinned SDK dereferences
the missing factory. Codec registration and removal remain available. Empty
playout still uses the actual stock engine. Internally handled CN, DTMF and RED
retain their native behavior; RED does not supply a missing ordinary decoder.
The deprecated stock decoder-format getter returns None in this pin. Current
format inspection also returns None for internally handled formats, whose native
getter otherwise dereferences a nonexistent AudioDecoder.

Factory callback reentry on the same thread returns `ReentrantCall` before
locking. An ordinary rejection or reentry does not disable later factory calls;
callback panic does. Product callback reentry latches an observable failure and
fails closed without inventing initialized audio or output-slot writes. Different
registrations remain independently callable, and concurrent factory callbacks
are serialized.
