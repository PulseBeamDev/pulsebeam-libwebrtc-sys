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
    audio_arrivals: Vec<SessionReceiverId>,
    all_audio: Vec<(usize, SessionReceiverId, SessionTransceiverId)>,
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
                    SessionEvent::Track {
                        receiver,
                        transceiver,
                    } => match session.receiver_media_type(receiver).unwrap() {
                        MediaType::Audio => {
                            self.all_audio.push((index, receiver, transceiver));
                            if index == 1 {
                                self.audio = Some(receiver);
                                self.audio_arrivals.push(receiver);
                            }
                        }
                        MediaType::Video if index == 1 => self.video = Some(receiver),
                        MediaType::Video => {}
                        kind => panic!("unexpected track kind: {kind:?}"),
                    },
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
        let _ = self.negotiate_answer_source(session, peers, None);
    }
    fn negotiate_answer_source(
        &mut self,
        session: &mut ProductionSession,
        peers: [SessionPeerId; 2],
        answer_source: Option<SessionSourceId>,
    ) -> Option<SessionSenderId> {
        let operation = session.create_offer(peers[0], false).unwrap();
        let offer = self.finish(session, peers, peers[0], operation).unwrap();
        let offer = self.gather(session, peers, 0, offer);
        let operation = session.set_remote_description(peers[1], offer).unwrap();
        self.finish(session, peers, peers[1], operation);
        let answer_sender = answer_source.map(|source| {
            let transceiver = self
                .all_audio
                .iter()
                .find(|(index, _, _)| *index == 1)
                .unwrap()
                .2;
            let sender = session.sender(transceiver).unwrap();
            session
                .replace_audio_source(sender, Some(source), "tone-1")
                .unwrap();
            session
                .set_direction(transceiver, RtpTransceiverDirection::SendReceive)
                .unwrap();
            sender
        });
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
        answer_sender
    }
}

fn drive_pcm_tones(
    session: &mut ProductionSession,
    call: &mut Call,
    peers: [SessionPeerId; 2],
    sources: [SessionSourceId; 2],
) -> Vec<DecodedAudioFrame> {
    let tones = [1000, 1500].map(|frequency| {
        let samples = (0..480)
            .map(|index| {
                if index * frequency * 2 / 48000 % 2 == 0 {
                    12000
                } else {
                    -12000
                }
            })
            .collect();
        AudioPcmFrame::i16_interleaved(48000, 1, 0, samples).unwrap()
    });
    let periods = [48, 32];
    let mut received = [false; 2];
    let mut frames = Vec::new();
    let start = Instant::now();
    let mut next = Duration::ZERO;
    while received != [true; 2] {
        let elapsed = start.elapsed();
        if elapsed >= next {
            for index in 0..2 {
                let capture = session.pcm_capture_time_now(sources[index]).unwrap();
                session
                    .push_pcm_at(sources[index], &tones[index], capture)
                    .unwrap();
            }
            next = elapsed + Duration::from_millis(10);
        }
        call.drain(session, peers);
        for &(peer_index, receiver, _) in &call.all_audio {
            if let Some(frame) = session.try_pcm_frame(receiver).unwrap() {
                let correlation = |lag: usize| {
                    frame
                        .samples
                        .iter()
                        .zip(frame.samples.iter().skip(lag))
                        .fold((0i64, 0i64), |(dot, energy), (&a, &b)| {
                            (
                                dot + i64::from(a) * i64::from(b),
                                energy + i64::from(a) * i64::from(a),
                            )
                        })
                };
                let period = periods[1 - peer_index];
                let (dot, energy) = correlation(period);
                let (opposite, half_energy) = correlation(period / 2);
                if energy > 1_000_000 && dot > energy / 2 && opposite < -half_energy / 2 {
                    assert_eq!(
                        (
                            frame.sample_rate_hz,
                            frame.channels,
                            frame.samples_per_channel
                        ),
                        (48000, 1, 480)
                    );
                    received[peer_index] = true;
                    frames.push(frame);
                }
            }
        }
        assert!(
            elapsed < Duration::from_secs(15),
            "bidirectional PCM missing: {received:?}"
        );
        thread::yield_now();
    }
    frames
}

#[test]
fn source_options_reconfigure_active_bidirectional_pcm_without_false_volume_gain() {
    let encoder = AudioEncoderFactory::builtin().unwrap();
    let decoder = AudioDecoderFactory::builtin_opus().unwrap();
    let mut session = ProductionSession::builder(ProductionSessionConfig::default())
        .audio_encoder_factory(&encoder)
        .audio_decoder_factory(&decoder)
        .build()
        .unwrap();
    let peers = [
        session.create_peer(PeerConfiguration::default()).unwrap(),
        session.create_peer(PeerConfiguration::default()).unwrap(),
    ];
    let sources = [
        session.create_pcm_source().unwrap(),
        session.create_pcm_source().unwrap(),
    ];
    let transceiver = session
        .publish_pcm(
            peers[0],
            sources[0],
            "tone-0",
            RtpTransceiverDirection::SendReceive,
        )
        .unwrap();
    let first_sender = session.sender(transceiver).unwrap();
    let (session, call, warm, senders) = thread::spawn(move || {
        let mut session = session;
        let mut call = Call::default();
        let answer_sender = call
            .negotiate_answer_source(&mut session, peers, Some(sources[1]))
            .unwrap();
        assert_eq!(call.all_audio.len(), 2);
        for &(_, receiver, _) in &call.all_audio {
            session.attach_pcm(receiver).unwrap();
        }
        let warm = drive_pcm_tones(&mut session, &mut call, peers, sources);
        (session, call, warm, [first_sender, answer_sender])
    })
    .join()
    .unwrap();
    let (session, later) = thread::spawn(move || {
        let mut session = session;
        let mut call = call;
        let options = AudioSourceOptions {
            // Source options reach only the send channel, not NetEq. Preserve
            // the full value rather than pretending to set receive capacity.
            audio_jitter_buffer_max_packets: Some(i32::MAX),
            audio_jitter_buffer_fast_accelerate: Some(false),
            audio_jitter_buffer_min_delay_ms: Some(0),
            stereo_swapping: Some(false),
            audio_network_adaptor: Some(false),
            audio_network_adaptor_config: Some(vec![0, 0xff, 0]),
            init_recording_on_send: Some(false),
            ..AudioSourceOptions::default()
        };
        for index in 0..2 {
            session
                .set_audio_source_options(sources[index], &options)
                .unwrap();
            assert_eq!(
                session.audio_source_options(sources[index]).unwrap(),
                options
            );
            let mut rejected = options.clone();
            rejected.noise_suppression_mode = Some(AudioProcessingMode::Unknown(99));
            rejected.stereo_swapping = Some(true);
            assert!(
                session
                    .set_audio_source_options(sources[index], &rejected)
                    .is_err()
            );
            assert_eq!(
                session.audio_source_options(sources[index]).unwrap(),
                options
            );
            assert!(
                session
                    .set_audio_source_volume(sources[index], f64::NAN)
                    .is_err()
            );
            session
                .set_audio_source_volume(sources[index], 0.0)
                .unwrap();
            session.set_audio_track_volume(senders[index], 0.0).unwrap();
            assert_eq!(session.audio_source_volume(sources[index]).unwrap(), 1.0);
            assert_eq!(
                session
                    .audio_track_status(senders[index])
                    .unwrap()
                    .unwrap()
                    .volume,
                1.0
            );
            // Native source notification alone does not notify an unchanged
            // live track. Reattach through SetTrack so SetSend genuinely reads
            // the new options on the actual send channel.
            session
                .replace_audio_source(
                    senders[index],
                    Some(sources[index]),
                    &format!("retuned-{index}"),
                )
                .unwrap();
        }
        let later = drive_pcm_tones(&mut session, &mut call, peers, sources);
        for source in sources {
            session
                .set_audio_source_options(source, &AudioSourceOptions::default())
                .unwrap();
            assert_eq!(
                session.audio_source_options(source).unwrap(),
                AudioSourceOptions::default()
            );
            session
                .push_pcm(
                    source,
                    &AudioPcmFrame::i16_interleaved(48000, 1, 0, vec![99; 480]).unwrap(),
                )
                .unwrap();
        }
        (session, later)
    })
    .join()
    .unwrap();
    thread::spawn(move || {
        let mut session = session;
        session.shutdown().unwrap();
        drop(session);
        assert!(!warm.is_empty() && !later.is_empty());
        assert!(
            warm.iter()
                .chain(&later)
                .all(|frame| frame.samples.iter().any(|&sample| sample != 0))
        );
    })
    .join()
    .unwrap();
}

fn receive_encoded_audio(
    session: &mut ProductionSession,
    call: &mut Call,
    peers: [SessionPeerId; 2],
    expected: &[&[u8]],
) -> Vec<EncodedAudioFrame> {
    let mut frames = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !expected.iter().all(|payload| {
        frames
            .iter()
            .any(|frame: &EncodedAudioFrame| frame.data == *payload)
    }) {
        call.drain(session, peers);
        for receiver in &call.audio_arrivals {
            while let Some(frame) = session.try_audio_frame(*receiver).unwrap() {
                frames.push(frame);
            }
        }
        assert!(
            Instant::now() < deadline,
            "encoded audio did not arrive: {frames:?}"
        );
        thread::yield_now();
    }
    frames
}

#[test]
fn replacing_one_opus_source_preserves_native_sender_and_unrelated_payloads() {
    let decoder = AudioDecoderFactory::builtin_opus().unwrap();
    let mut session = ProductionSession::builder(ProductionSessionConfig::default())
        .audio_decoder_factory(&decoder)
        .build()
        .unwrap();
    let peers = [
        session.create_peer(PeerConfiguration::default()).unwrap(),
        session.create_peer(PeerConfiguration::default()).unwrap(),
    ];
    let sources = [
        session.create_opus_source(1).unwrap(),
        session.create_opus_source(1).unwrap(),
    ];
    let transceivers = sources.map(|source| {
        session
            .publish_opus(
                peers[0],
                source,
                &format!("opus-{}", source.get()),
                RtpTransceiverDirection::SendOnly,
            )
            .unwrap()
    });
    let senders = transceivers.map(|transceiver| session.sender(transceiver).unwrap());
    let old = vec![0xf8, 0x11, 0x01];
    let independent = vec![0xf8, 0x22, 0x02];
    let replacement = vec![0xf8, 0x33, 0x03];
    let (session, mut call, first, ssrcs, independent) = thread::spawn(move || {
        let mut session = session;
        let mut call = Call::default();
        call.negotiate(&mut session, peers);
        assert_eq!(call.audio_arrivals.len(), 2);
        for receiver in &call.audio_arrivals {
            session.attach_encoded_audio(*receiver).unwrap();
        }
        for source in sources {
            assert_eq!(
                session.audio_source_state(source).unwrap(),
                AudioSourceState::Live
            );
            assert!(!session.audio_source_is_remote(source).unwrap());
            assert_eq!(session.audio_source_volume(source).unwrap(), 1.0);
            assert_eq!(
                session.audio_source_options(source).unwrap(),
                AudioSourceOptions::default()
            );
        }
        for sender in senders {
            let status = session.audio_track_status(sender).unwrap().unwrap();
            assert_eq!(status.kind, "audio");
            assert_eq!(status.state, AudioTrackState::Live);
        }
        let ssrcs = senders.map(|sender| session.sender_ssrc(sender).unwrap());
        assert_ne!(ssrcs[0], ssrcs[1]);
        for ((source, data), level) in sources
            .into_iter()
            .zip([old.clone(), independent.clone()])
            .zip([
                OpusAudioLevel::new(17, true).unwrap(),
                OpusAudioLevel::new(83, false).unwrap(),
            ])
        {
            session
                .push_opus_with_audio_level(
                    source,
                    &OpusInputFrame {
                        data,
                        rtp_timestamp: 0,
                        samples_per_channel: 960,
                    },
                    level,
                )
                .unwrap();
        }
        let frames = receive_encoded_audio(&mut session, &mut call, peers, &[&old, &independent]);
        assert!(
            frames
                .iter()
                .any(|frame| frame.data == old && frame.ssrc == ssrcs[0])
        );
        assert!(
            frames
                .iter()
                .any(|frame| frame.data == independent && frame.ssrc == ssrcs[1])
        );
        let previous = frames.iter().find(|frame| frame.data == old).unwrap();
        assert_eq!(
            (previous.audio_level_dbov, previous.voice_activity),
            (Some(17), Some(true))
        );
        let unrelated = frames
            .iter()
            .find(|frame| frame.data == independent)
            .unwrap();
        assert_eq!(
            (unrelated.audio_level_dbov, unrelated.voice_activity),
            (Some(83), Some(false))
        );
        (session, call, frames, ssrcs, independent)
    })
    .join()
    .unwrap();
    let (session, second) = thread::spawn(move || {
        let mut session = session;
        // Leave old input queued at the same native sender while replacing its
        // producer. Already accepted output may retain the old payload.
        session
            .push_opus(
                sources[0],
                &OpusInputFrame {
                    data: vec![0xf8, 0x11, 0x01],
                    rtp_timestamp: 960,
                    samples_per_channel: 960,
                },
            )
            .unwrap();
        let new_source = session.create_opus_source(1).unwrap();
        session
            .replace_audio_source(senders[0], Some(new_source), "new-opus")
            .unwrap();
        assert_eq!(
            session.audio_track_status(senders[0]).unwrap().unwrap().id,
            "new-opus"
        );
        session.close_source(sources[0]).unwrap();
        assert!(
            session
                .push_opus(
                    sources[0],
                    &OpusInputFrame {
                        data: replacement.clone(),
                        rtp_timestamp: 1920,
                        samples_per_channel: 960
                    }
                )
                .is_err()
        );
        session
            .push_opus_with_audio_level(
                new_source,
                &OpusInputFrame {
                    data: replacement.clone(),
                    rtp_timestamp: 1920,
                    samples_per_channel: 960,
                },
                OpusAudioLevel::new(0, false).unwrap(),
            )
            .unwrap();
        session
            .push_opus_with_audio_level(
                sources[1],
                &OpusInputFrame {
                    data: independent.clone(),
                    rtp_timestamp: 960,
                    samples_per_channel: 960,
                },
                OpusAudioLevel::new(83, false).unwrap(),
            )
            .unwrap();
        let frames = receive_encoded_audio(
            &mut session,
            &mut call,
            peers,
            &[&replacement, &independent],
        );
        assert!(
            frames
                .iter()
                .any(|frame| frame.data == replacement && frame.ssrc == ssrcs[0])
        );
        assert!(
            frames
                .iter()
                .any(|frame| frame.data == independent && frame.ssrc == ssrcs[1])
        );
        let current = frames
            .iter()
            .find(|frame| frame.data == replacement)
            .unwrap();
        assert_eq!(
            (current.audio_level_dbov, current.voice_activity),
            (Some(0), Some(false))
        );
        let unrelated = frames
            .iter()
            .find(|frame| frame.data == independent)
            .unwrap();
        assert_eq!(
            (unrelated.audio_level_dbov, unrelated.voice_activity),
            (Some(83), Some(false))
        );
        assert_eq!(
            senders.map(|sender| session.sender_ssrc(sender).unwrap()),
            ssrcs
        );
        session
            .replace_audio_source(senders[0], None, "unused")
            .unwrap();
        assert_eq!(session.audio_track_status(senders[0]).unwrap(), None);
        session
            .replace_audio_source(senders[0], Some(new_source), "reattached-opus")
            .unwrap();
        session
            .push_opus(
                new_source,
                &OpusInputFrame {
                    data: replacement,
                    rtp_timestamp: 2880,
                    samples_per_channel: 960,
                },
            )
            .unwrap();
        (session, frames)
    })
    .join()
    .unwrap();
    thread::spawn(move || {
        let mut session = session;
        session.shutdown().unwrap();
        drop(session);
        assert!(first.iter().any(|frame| frame.data == [0xf8, 0x11, 0x01]));
        assert!(second.iter().any(|frame| frame.data == [0xf8, 0x33, 0x03]));
    })
    .join()
    .unwrap();
    assert_eq!(decoder.decoder_statistics().unwrap().input_packets, 0);
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
    let audio_transceiver = session
        .publish_pcm(peers[0], pcm, "pcm", RtpTransceiverDirection::SendOnly)
        .unwrap();
    let audio_sender = session.sender(audio_transceiver).unwrap();
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
                let capture_time = session.pcm_capture_time_now(pcm).unwrap();
                session.push_pcm_at(pcm, &pcm_frame, capture_time).unwrap();
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
        let replacement_pcm = session.create_pcm_source().unwrap();
        session
            .replace_audio_source(audio_sender, Some(replacement_pcm), "replacement-pcm")
            .unwrap();
        assert_eq!(
            session
                .audio_track_status(audio_sender)
                .unwrap()
                .unwrap()
                .id,
            "replacement-pcm"
        );
        let replacement_samples = (0..441)
            .map(|index| {
                if index * 3000 / 44100 % 2 == 0 {
                    12000
                } else {
                    -12000
                }
            })
            .collect();
        let replacement_frame =
            AudioPcmFrame::i16_interleaved(44100, 1, 0, replacement_samples).unwrap();
        session.close_source(pcm).unwrap();
        assert!(session.push_pcm(pcm, &pcm_frame).is_err());
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
        let mut next_audio = Duration::ZERO;
        let mut saw_video = false;
        let mut saw_audio = false;
        while !saw_video || !saw_audio {
            let elapsed = start.elapsed();
            if elapsed >= next_audio {
                let capture_time = session.pcm_capture_time_now(replacement_pcm).unwrap();
                session
                    .push_pcm_at(replacement_pcm, &replacement_frame, capture_time)
                    .unwrap();
                next_audio = elapsed + Duration::from_millis(10);
            }
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
                saw_video |= (195..=205).contains(&average);
            }
            if let Some(frame) = session.try_pcm_frame(call.audio.unwrap()).unwrap() {
                // The 44.1 kHz replacement carries 1500 Hz, unlike the retired
                // 1000 Hz input. Observe native resampling and actual new media,
                // not silence or PLC decay after detaching the old producer.
                let correlation = |lag: usize| {
                    frame
                        .samples
                        .iter()
                        .zip(frame.samples.iter().skip(lag))
                        .fold((0i64, 0i64), |(dot, energy), (&a, &b)| {
                            (
                                dot + i64::from(a) * i64::from(b),
                                energy + i64::from(a) * i64::from(a),
                            )
                        })
                };
                let (period, energy) = correlation(32);
                let (opposite, opposite_energy) = correlation(48);
                saw_audio |=
                    energy > 1_000_000 && period > energy / 2 && opposite < -opposite_energy / 2;
            }
            assert!(
                Instant::now() < deadline,
                "replacement media missing: video={saw_video}, audio={saw_audio}"
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
