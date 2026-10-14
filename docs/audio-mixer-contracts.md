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

This milestone is the standalone stock mixer and source callback contract. It
does not yet provide peer-factory/ProductionSession mixer selection or an owning
caller implementation of the entire mixer interface.
