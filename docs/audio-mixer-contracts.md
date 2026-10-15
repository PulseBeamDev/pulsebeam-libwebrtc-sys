# Owning standalone mixer

`AudioMixer::stock` creates the pinned SDK `AudioMixerImpl` with its default
output-rate calculator behavior and limiter. There is no environment input in
this native creation contract. This is software mixing, not device activation.

`AudioMixerSource::from_provider` creates an owning source identity. Clones name
the same participant, independent of SSRC values. Successful registration retains
the source until removal or mixer destruction. Duplicate addition returns false;
absent removal returns false without invoking the asserting SDK removal call.
The SDK mixer is destroyed before retained participants are released.

Every mix consumes current preferred-rate answers. Native rate selection uses
the maximum preference and rounds it up to a native rate. Only the actually
reached maximum-rate assertion domain is guarded. Individual preferences are
not normalized or whitelisted. A maximum outside 8–48 kHz refuses the owning
operation before source frames are consumed. Because native `Mix` has no error
return, its internal invocation completes at a safe rate with sources omitted;
that output is discarded and never exposed as successful audio.

Providers return native normal, muted or error dispositions. Normal/muted frames
must have exactly the requested 10-ms rate and one or two channels. Normal frames
must expose a nonmuted initialized prefix. Muted samples are not consumed;
ordinary native source error skips that source without inventing a callback
failure. Malformed frames, callback rejection, panic and recursive source calls
produce observable failures, never a zero-output successful owning operation.
The independent first-failure observer remains usable after source retirement.

The actual SDK combiner writes an initially fresh native frame. Its output is
validated before conversion to the owning `AudioFrame`; only unused owned tail
storage is initialized afterward. Native prefix, metadata and mute state are
preserved. Empty/muted/error-only native mixes remain genuinely muted.

The mixer is sequence-bound and cannot be sent or shared. Source handles are
Send/Sync, but their Send-only providers are mutex-serialized, including when
registered with multiple mixers. A thread-local source scope rejects recursive
callbacks before mutex acquisition. Nested mixer operations during native mixing
are rejected before the SDK mixer mutex.

## Factory and session installation

PeerConnectionFactoryBuilder::audio_mixer consumes the owning mixer and its
registered sources. ProductionSessionBuilder::audio_mixer_sources constructs and
installs a mixer from Send source products inside the actor without accepting a
sequence-bound standalone handle. The native installed adapter delegates to the
exact selected SDK mixer and retains its shared source/observer state through
final native release. It does not create a replacement engine or a second mixer.

AudioMixer::failure_observer, factory and session accessors retain the first
native mix failure independently of source observers. Direct SDK invocation gets
a mix scope too, preserving preferred-rate guards and callback failure tracking.
Native Mix is void: an installed failure can only deliver a safe muted frame and
observer notification with this adapter. That is an explicit publication-channel
limitation, not fail-stop installed custom PCM or successful custom output.
AudioMixer::from_provider supplies an owning caller implementation of the entire
mixer interface.

## Caller mixer implementation

AudioMixerProvider supplies the entire selected native mixer, not a callback on a
stock mixer. Its native registration tokens retain exact registration-epoch
identity across clones. A removed source cannot be called through an old token,
even if another source later occupies the same address. Add/remove info loans
allow current SSRC/preferred-rate queries; the mix loan additionally drives the
real registered SDK sources. No raw source or callable loan escapes publicly.
Sources can be added/removed on other threads, serialized against the entire mix
callback. Returning native false from AddSource is distinct from Err/panic.
RemoveSource retires raw-source access even if user removal fails. A provider
keeps tokens, not SDK source ownership; the SDK owns its receive streams.

ProductionSessionBuilder::audio_mixer_provider takes a Send provider and builds
the sequence-bound native product inside the actor. This selector and
audio_mixer_sources are last-choice-wins. Valid caller frames preserve actual
native prefix/metadata/mute; failed or malformed standalone mixes return errors
without manufacturing audio. Installed void Mix still has the publication gap
above; safe muted failure-drain metadata is not a successful custom result.
