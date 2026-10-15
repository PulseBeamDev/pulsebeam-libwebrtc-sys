//! Final-owner cleanup cannot synchronously destroy its current SDK role.
use super::*;
use std::{
    ops::Deref,
    sync::{OnceLock, mpsc},
};

pub(super) struct CaptiveFactory(Option<PeerConnectionFactory>);
impl CaptiveFactory {
    pub(super) fn new(factory: PeerConnectionFactory) -> Self {
        Self(Some(factory))
    }
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_none()
    }
    fn take(&mut self) -> Self {
        Self(self.0.take())
    }
}
impl Deref for CaptiveFactory {
    type Target = PeerConnectionFactory;
    fn deref(&self) -> &Self::Target {
        self.0
            .as_ref()
            .expect("factory is moved only during final teardown")
    }
}
static CLEANUP: OnceLock<mpsc::Sender<ProductionSession>> = OnceLock::new();

/// Initialize before constructing a factory, so teardown never has to create
/// a thread (and cannot recursively drop its payload on a spawn failure).
pub(super) fn prepare() -> Result<(), PeerError> {
    if CLEANUP.get().is_some() {
        return Ok(());
    }
    let (sender, receiver) = mpsc::channel::<ProductionSession>();
    std::thread::Builder::new()
        .name("webrtc-cleanup".into())
        .spawn(move || {
            while let Ok(owner) = receiver.recv() {
                // User-owned provider destructors must not kill future cleanup.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(owner)));
            }
        })
        .map_err(|e| {
            error(
                PeerErrorKind::NativeConstruction,
                &format!("session cleanup executor: {e}"),
            )
        })?;
    // Concurrent constructors may race. The losing channel is closed and its
    // empty worker exits; only the published worker ever receives resources.
    let _ = CLEANUP.set(sender);
    Ok(())
}
pub(super) fn defer(owner: ProductionSession) {
    let sender = CLEANUP.get().expect("prepared before factory construction");
    if let Err(failed) = sender.send(owner) {
        // This requires the persistent executor to have terminated despite
        // panic containment. Never free live SDK resources from its callback.
        eprintln!("session cleanup executor unavailable; retaining native resources for safety");
        std::mem::forget(failed.0);
    }
}
impl ProductionSession {
    pub(super) fn take_for_cleanup(&mut self) -> Self {
        // Move every Rc-containing field, not clones. Construction requires all
        // fields, so future resources cannot silently be omitted from handoff.
        Self {
            connectivity: std::mem::take(&mut self.connectivity),
            sframe_encrypters: std::mem::take(&mut self.sframe_encrypters),
            sframe_decrypters: std::mem::take(&mut self.sframe_decrypters),
            frame_encryptors: std::mem::take(&mut self.frame_encryptors),
            frame_decryptors: std::mem::take(&mut self.frame_decryptors),
            sender_observers: std::mem::take(&mut self.sender_observers),
            receiver_observers: std::mem::take(&mut self.receiver_observers),
            channels: std::mem::take(&mut self.channels),
            video_sinks: std::mem::take(&mut self.video_sinks),
            audio_sinks: std::mem::take(&mut self.audio_sinks),
            pcm_sinks: std::mem::take(&mut self.pcm_sinks),
            track_observers: std::mem::take(&mut self.track_observers),
            stream_observers: std::mem::take(&mut self.stream_observers),
            media_streams: std::mem::take(&mut self.media_streams),
            track_sources: std::mem::take(&mut self.track_sources),
            audio_processors: std::mem::take(&mut self.audio_processors),
            media_tracks: std::mem::take(&mut self.media_tracks),
            transceivers: std::mem::take(&mut self.transceivers),
            senders: std::mem::take(&mut self.senders),
            receivers: std::mem::take(&mut self.receivers),
            sources: std::mem::take(&mut self.sources),
            peers: std::mem::take(&mut self.peers),
            input: self.input.take(),
            factory: self.factory.take(),
            readiness: self.readiness.clone(),
            closed: self.closed,
            _exclusive: PhantomData,
        }
    }
}
