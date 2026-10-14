use pulsebeam_webrtc_sys::*;
use std::{
    collections::HashMap,
    thread,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Call {
    operations: HashMap<(SessionPeerId, OperationId), Option<SessionDescription>>,
    gathered: [bool; 2],
    connected: [bool; 2],
    audio: Option<SessionReceiverId>,
    video: Option<SessionReceiverId>,
}
impl Call {
    fn drain(&mut self, session: &mut ProductionSession, peers: [SessionPeerId; 2]) {
        for (index, peer) in peers.into_iter().enumerate() {
            while let Some(event) = session.try_peer_event(peer).unwrap() {
                match event {
                    SessionEvent::OperationComplete(done) => {
                        self.operations
                            .insert((peer, done.operation_id), done.result.unwrap());
                    }
                    SessionEvent::Track { receiver, .. } if index == 1 => {
                        match session.receiver_media_type(receiver).unwrap() {
                            MediaType::Audio => self.audio = Some(receiver),
                            MediaType::Video => self.video = Some(receiver),
                            kind => panic!("unexpected track kind: {kind:?}"),
                        }
                    }
                    SessionEvent::IceGatheringStateChanged(IceGatheringState::Complete) => {
                        self.gathered[index] = true
                    }
                    SessionEvent::ConnectionStateChanged(ConnectionState::Connected) => {
                        self.connected[index] = true
                    }
                    _ => {}
                }
            }
        }
    }
    fn finish(
        &mut self,
        session: &mut ProductionSession,
        peers: [SessionPeerId; 2],
        peer: SessionPeerId,
        operation: OperationId,
    ) -> Option<SessionDescription> {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.drain(session, peers);
            if let Some(result) = self.operations.remove(&(peer, operation)) {
                return result;
            }
            assert!(
                Instant::now() < deadline,
                "native operation did not complete"
            );
            thread::yield_now();
        }
    }
    fn gather(
        &mut self,
        session: &mut ProductionSession,
        peers: [SessionPeerId; 2],
        index: usize,
        description: SessionDescription,
    ) -> SessionDescription {
        let operation = session
            .set_local_description(peers[index], description)
            .unwrap();
        self.finish(session, peers, peers[index], operation);
        let deadline = Instant::now() + Duration::from_secs(15);
        while !self.gathered[index] {
            self.drain(session, peers);
            assert!(Instant::now() < deadline, "ICE gathering did not complete");
            thread::yield_now();
        }
        let descriptions = session.descriptions(peers[index]).unwrap();
        descriptions
            .current_local
            .or(descriptions.pending_local)
            .unwrap()
    }
    fn negotiate(&mut self, session: &mut ProductionSession, peers: [SessionPeerId; 2]) {
        let operation = session.create_offer(peers[0], false).unwrap();
        let offer = self.finish(session, peers, peers[0], operation).unwrap();
        let offer = self.gather(session, peers, 0, offer);
        let operation = session.set_remote_description(peers[1], offer).unwrap();
        self.finish(session, peers, peers[1], operation);
        let operation = session.create_answer(peers[1]).unwrap();
        let answer = self.finish(session, peers, peers[1], operation).unwrap();
        let answer = self.gather(session, peers, 1, answer);
        let operation = session.set_remote_description(peers[0], answer).unwrap();
        self.finish(session, peers, peers[0], operation);
        let deadline = Instant::now() + Duration::from_secs(15);
        while self.connected != [true; 2] {
            self.drain(session, peers);
            assert!(Instant::now() < deadline, "native peers did not connect");
            thread::yield_now();
        }
    }
}

fn video(luma: u8, timestamp_us: i64) -> VideoFrame {
    let mut pixels = vec![128; 64 * 48 * 3 / 2];
    pixels[..64 * 48].fill(luma);
    VideoFrame::i420(64, 48, pixels, timestamp_us, 0).unwrap()
}

#[test]
fn raw_pcm_requires_an_explicit_pcm_encoder_without_changing_encoded_defaults() {
    let mut session = ProductionSession::new(ProductionSessionConfig::default()).unwrap();
    assert_eq!(
        session.create_pcm_source().unwrap_err().kind,
        PeerErrorKind::UnsupportedOperation
    );
    assert!(session.create_opus_source(1).is_ok());
    session.shutdown().unwrap();
}

#[test]
fn raw_media_crosses_native_call_and_retains_owned_output_after_replacement_and_migration() {
    let audio_encoder = AudioEncoderFactory::builtin().unwrap();
    let audio_decoder = AudioDecoderFactory::builtin().unwrap();
    let video_encoder = VideoEncoderFactoryHandle::builtin().unwrap();
    let video_decoder = VideoDecoderFactoryHandle::builtin_vp8().unwrap();
    let mut session = ProductionSession::builder(ProductionSessionConfig::default())
        .audio_encoder_factory(&audio_encoder)
        .audio_decoder_factory(&audio_decoder)
        .video_encoder_factory(&video_encoder)
        .video_decoder_factory(&video_decoder)
        .build()
        .unwrap();
    drop((audio_encoder, audio_decoder, video_encoder, video_decoder));
    let peers = [
        session.create_peer(PeerConfiguration::default()).unwrap(),
        session.create_peer(PeerConfiguration::default()).unwrap(),
    ];
    let pcm = session.create_pcm_source().unwrap();
    let raw = session.create_raw_video_source().unwrap();
    session
        .publish_pcm(peers[0], pcm, "pcm", RtpTransceiverDirection::SendOnly)
        .unwrap();
    let transceiver = session
        .publish_video(peers[0], raw, "raw", RtpTransceiverDirection::SendOnly, &[])
        .unwrap();
    let sender = session.sender(transceiver).unwrap();
    // Wrong-kind operations reject before handing input to a native source.
    assert!(session.push_raw_video(pcm, &video(40, 0)).is_err());
    assert!(
        session
            .publish_pcm(peers[0], raw, "wrong", RtpTransceiverDirection::SendOnly)
            .is_err()
    );

    let (session, mut call) = thread::spawn(move || {
        let mut session = session;
        let mut call = Call::default();
        call.negotiate(&mut session, peers);
        session.attach_pcm(call.audio.unwrap()).unwrap();
        session.attach_raw_video(call.video.unwrap()).unwrap();
        assert!(session.attach_encoded_audio(call.audio.unwrap()).is_err());
        assert!(session.attach_encoded_video(call.video.unwrap()).is_err());
        (session, call)
    })
    .join()
    .unwrap();

    let (session, first_video, first_audio) = thread::spawn(move || {
        let mut session = session;
        let samples: Vec<i16> = (0..480)
            .map(|i| if i % 48 < 24 { 12000 } else { -12000 })
            .collect();
        let pcm_frame = AudioPcmFrame::i16_interleaved(48000, 1, 0, samples).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let start = Instant::now();
        let mut next_audio = Duration::ZERO;
        let mut next_video = Duration::ZERO;
        let mut audio = None;
        let mut image = None;
        while audio.is_none() || image.is_none() {
            let elapsed = start.elapsed();
            if elapsed >= next_audio {
                session.push_pcm(pcm, &pcm_frame).unwrap();
                next_audio = elapsed + Duration::from_millis(10);
            }
            if elapsed >= next_video {
                let capture_us = session.clock_timestamp().unwrap().microseconds().unwrap();
                session.push_raw_video(raw, &video(40, capture_us)).unwrap();
                next_video = elapsed + Duration::from_millis(33);
            }
            call.drain(&mut session, peers);
            if let Some(frame) = session.try_pcm_frame(call.audio.unwrap()).unwrap() {
                // Ignore startup concealment/silence. A real nonzero source
                // must survive the native encoder, RTP and decoder paths.
                if frame
                    .samples
                    .iter()
                    .any(|sample| sample.unsigned_abs() > 2000)
                {
                    audio = Some(frame);
                }
            }
            if let Some(frame) = session.try_raw_video_frame(call.video.unwrap()).unwrap() {
                image = Some(frame);
            }
            assert!(Instant::now() < deadline, "raw media was not decoded");
            thread::yield_now();
        }
        let audio = audio.unwrap();
        assert_eq!((audio.sample_rate_hz, audio.channels), (48000, 1));
        assert_eq!(audio.samples.len(), audio.samples_per_channel as usize);
        let image = image.unwrap();
        assert_eq!((image.width, image.height), (64, 48));
        let average = image.buffer.as_bytes()[..64 * 48]
            .iter()
            .map(|&v| u64::from(v))
            .sum::<u64>()
            / (64 * 48);
        assert!(
            (35..=45).contains(&average),
            "unexpected decoded luma {average}"
        );

        let replacement = session.create_raw_video_source().unwrap();
        session
            .replace_video_source(sender, Some(replacement), "replacement")
            .unwrap();
        session.close_source(raw).unwrap();
        assert!(session.push_raw_video(raw, &video(40, 0)).is_err());
        // Closing and replacing sinks must release their native reservation.
        session.close_media_sink(call.video.unwrap()).unwrap();
        session.attach_raw_video(call.video.unwrap()).unwrap();
        session.close_media_sink(call.audio.unwrap()).unwrap();
        session.attach_pcm(call.audio.unwrap()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let start = Instant::now();
        let mut next_video = Duration::ZERO;
        loop {
            let elapsed = start.elapsed();
            if elapsed >= next_video {
                let capture_us = session.clock_timestamp().unwrap().microseconds().unwrap();
                session
                    .push_raw_video(replacement, &video(200, capture_us))
                    .unwrap();
                next_video = elapsed + Duration::from_millis(33);
            }
            call.drain(&mut session, peers);
            if let Some(frame) = session.try_raw_video_frame(call.video.unwrap()).unwrap() {
                let average = frame.buffer.as_bytes()[..64 * 48]
                    .iter()
                    .map(|&v| u64::from(v))
                    .sum::<u64>()
                    / (64 * 48);
                if (195..=205).contains(&average) {
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "replacement source was not decoded"
            );
            thread::yield_now();
        }
        // Leave queued native work for migrated final shutdown.
        let capture_us = session.clock_timestamp().unwrap().microseconds().unwrap();
        session
            .push_raw_video(replacement, &video(200, capture_us))
            .unwrap();
        (session, image, audio)
    })
    .join()
    .unwrap();
    thread::spawn(move || {
        let mut session = session;
        session.shutdown().unwrap();
        session.shutdown().unwrap();
        drop(session);
        // Caller-owned output has no session, sink or native-buffer borrow.
        assert!((35..=45).contains(&first_video.buffer.as_bytes()[0]));
        assert!(
            first_audio
                .samples
                .iter()
                .any(|sample| sample.unsigned_abs() > 2000)
        );
    })
    .join()
    .unwrap();
}
