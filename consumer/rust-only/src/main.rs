use std::time::Duration;

fn main() {
    let ice = pulsebeam_webrtc_sys::IceConfig::default();
    ice.validate().unwrap();
    assert!(ice.receiving_timeout.is_none());
    assert!(ice.resolved_defaults().receiving_timeout.is_some());
    let candidate = pulsebeam_webrtc_sys::TransportCandidate::parse(
        b"candidate:consumer 1 udp 2122260223 127.0.0.1 43210 typ host generation 0",
    )
    .unwrap();
    candidate.verify().unwrap();
    assert_eq!(candidate.properties().foundation, b"consumer");
    assert_eq!(candidate.properties().id, candidate.clone().properties().id);
    assert_eq!(
        pulsebeam_webrtc_sys::IcePacketOptions::default().packet_id,
        -1
    );
    assert_eq!(
        pulsebeam_webrtc_sys::bridge_identity(),
        "pulsebeam-webrtc-sys-bridge-v2"
    );
    let clock = pulsebeam_webrtc_sys::ManualClock::new(Duration::from_secs(7)).unwrap();
    let environment = pulsebeam_webrtc_sys::Environment::builder()
        .clock(&clock)
        .build()
        .unwrap();
    assert_eq!(environment.now(), Duration::from_secs(7));
    clock.advance(Duration::from_millis(5)).unwrap();
    assert_eq!(environment.now(), Duration::from_millis(7_005));
    {
        let encoder = pulsebeam_webrtc_sys::VideoEncoderFactoryHandle::builtin().unwrap();
        let decoder = pulsebeam_webrtc_sys::VideoDecoderFactoryHandle::builtin().unwrap();
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            let format = pulsebeam_webrtc_sys::VideoCodecFormat::new("H264")
                .with_parameter("profile-level-id", "42e01f")
                .with_parameter("packetization-mode", "1")
                .with_parameter("level-asymmetry-allowed", "1");
            assert!(
                encoder
                    .supported_formats()
                    .iter()
                    .any(|format| format.name == "H264")
            );
            assert!(
                decoder
                    .supported_formats()
                    .iter()
                    .any(|format| format.name == "H264")
            );
            assert!(encoder.query_support(&format, None, None).supported);
            assert!(decoder.query_support(&format, false, None).supported);
        }
        let factory = pulsebeam_webrtc_sys::PeerConnectionFactory::builder()
            .environment(environment)
            .video_encoder_factory(encoder)
            .video_decoder_factory(decoder)
            .build()
            .unwrap();
        let stream = factory
            .create_local_media_stream(b"cold-stream\0\xff")
            .unwrap();
        assert_eq!(stream.id(), b"cold-stream\0\xff");
        assert!(stream.audio_tracks().is_empty());
        let mut peer = factory
            .create_peer_connection(pulsebeam_webrtc_sys::PeerConfiguration::default())
            .unwrap();
        assert!(peer.senders().is_empty());
        assert!(peer.receivers().is_empty());
        assert!(peer.transceivers().unwrap().is_empty());
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        assert!(
            peer.video_sender_capabilities()
                .unwrap()
                .iter()
                .any(|codec| codec.format().name == "H264")
        );
        assert_eq!(
            peer.signaling_state(),
            pulsebeam_webrtc_sys::SignalingState::Stable
        );
        assert_eq!(
            peer.peer_connection_state(),
            pulsebeam_webrtc_sys::ConnectionState::New
        );
        assert_eq!(
            peer.ice_gathering_state(),
            pulsebeam_webrtc_sys::IceGatheringState::New
        );
        let channel = peer
            .create_data_channel("consumer", Default::default())
            .unwrap();
        assert_eq!(channel.label(), b"consumer");
        let bytes_channel = peer
            .create_data_channel_bytes(
                b"consumer\xff\0",
                Some(pulsebeam_webrtc_sys::DataChannelConfiguration {
                    reliable: true,
                    max_retransmit_time_ms: Some(-1),
                    max_retransmits: Some(i32::MAX),
                    protocol: b"protocol\x80\0".to_vec(),
                    ..Default::default()
                }),
            )
            .unwrap();
        assert_eq!(bytes_channel.label(), b"consumer\xff\0");
        assert_eq!(bytes_channel.configuration().protocol, b"protocol\x80\0");
        assert_eq!(bytes_channel.configuration().max_retransmits, Some(65535));
        assert!(!bytes_channel.reliable());
        assert!(
            peer.create_data_channel_bytes(b"native-default", None)
                .unwrap()
                .reliable()
        );
        let mut stats = pulsebeam_webrtc_sys::RtcStats::new(
            pulsebeam_webrtc_sys::RtcStatsClass::DataChannelStats,
            b"consumer-stat\xff\0",
            pulsebeam_webrtc_sys::Timestamp::zero(),
        )
        .unwrap();
        let pulsebeam_webrtc_sys::RtcStatsValues::DataChannelStats(mut values) = stats.values()
        else {
            panic!()
        };
        values.bytes_sent = Some(u64::MAX);
        assert!(
            stats.set_values(pulsebeam_webrtc_sys::RtcStatsValues::DataChannelStats(
                values
            ))
        );
        let mut report = pulsebeam_webrtc_sys::RtcStatsReportBuilder::new(
            pulsebeam_webrtc_sys::Timestamp::zero(),
        );
        assert!(report.add(stats));
        let report = report.freeze();
        std::thread::spawn(move || {
            assert!(
                report
                    .get_as(
                        b"consumer-stat\xff\0",
                        pulsebeam_webrtc_sys::RtcStatsClass::DataChannelStats
                    )
                    .is_some()
            );
            assert_eq!(report.len(), 1);
        })
        .join()
        .unwrap();
        let source = factory.create_audio_source().unwrap();
        let track = factory
            .create_audio_track("consumer-legacy-selector", &source)
            .unwrap()
            .as_media_track();
        assert_eq!(track.id(), b"consumer-legacy-selector");
        let pulsebeam_webrtc_sys::MediaTrackSource::Audio(native_source) = track.source().unwrap()
        else {
            panic!()
        };
        assert!(!native_source.remote());
        assert_eq!(
            native_source.state(),
            pulsebeam_webrtc_sys::MediaSourceState::Live
        );
        assert_eq!(native_source.options().echo_cancellation, None);
        let observation = track.observe(std::num::NonZeroUsize::new(2).unwrap());
        assert!(track.set_enabled(false));
        assert!(observation.take_event().is_some());
        assert!(observation.take_event().is_none());
        assert!(track.same_resource(&track.clone()));
        assert!(
            peer.request_legacy_stats_with_track(
                Some(&track),
                pulsebeam_webrtc_sys::LegacyStatsOutputLevel::Standard
            )
            .is_err()
        ); // The SDK has not remembered this unattached track.
        assert_eq!(peer.event_observation().admitted_operations, 0);
        let stats_id = peer
            .request_legacy_stats(pulsebeam_webrtc_sys::LegacyStatsOutputLevel::Standard)
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(pulsebeam_webrtc_sys::PeerConnectionEvent::LegacyStats(snapshot)) =
                peer.try_next_event()
            {
                if snapshot.operation_id == stats_id {
                    assert!(!snapshot.reports.is_empty());
                    break;
                }
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        let alias = channel.clone();
        assert!(channel.same_resource(&alias));
        let limits = pulsebeam_webrtc_sys::DataChannel::receive_queue_limits();
        assert!(limits.max_messages > 0 && limits.max_bytes > 0);
        drop(channel);
        assert_eq!(
            alias.state(),
            pulsebeam_webrtc_sys::DataChannelState::Connecting
        );
        peer.close().unwrap();
        assert_eq!(
            peer.signaling_state(),
            pulsebeam_webrtc_sys::SignalingState::Closed
        );
        assert_eq!(
            peer.peer_connection_state(),
            pulsebeam_webrtc_sys::ConnectionState::Closed
        );
        assert_eq!(
            alias.state(),
            pulsebeam_webrtc_sys::DataChannelState::Closed
        );
        let mut session = pulsebeam_webrtc_sys::ProductionSession::new(Default::default()).unwrap();
        let peer = session.create_peer(Default::default()).unwrap();
        assert!(session.peer_sctp_transport(peer).unwrap().is_none());
        assert!(
            session
                .peer_dtls_transport_by_mid(peer, b"absent")
                .unwrap()
                .is_none()
        );
        let source = session.create_opus_source(1).unwrap();
        let transceiver = session
            .publish_opus(
                peer,
                source,
                "consumer-audio",
                pulsebeam_webrtc_sys::RtpTransceiverDirection::SendOnly,
            )
            .unwrap();
        let sender = session.sender(transceiver).unwrap();
        let selected = session.sender_track_reference(sender).unwrap().unwrap();
        assert_eq!(session.media_track_id(selected).unwrap(), b"consumer-audio");
        assert_eq!(
            session.sender_track_reference(sender).unwrap(),
            Some(selected)
        );
        let selected_source = session.media_track_source(selected).unwrap().unwrap();
        let dtmf = session.sender_dtmf(sender).unwrap().unwrap();
        assert_eq!(session.sender_dtmf(sender).unwrap(), Some(dtmf));
        assert!(!session.dtmf_can_insert_tones(dtmf).unwrap());
        assert!(
            session
                .register_dtmf_observer(dtmf, std::num::NonZeroUsize::new(2).unwrap())
                .unwrap()
        );
        std::thread::spawn(move || {
            assert_eq!(
                session.media_track_kind(selected).unwrap(),
                pulsebeam_webrtc_sys::MediaTrackKind::Audio
            );
            assert!(!session.track_source_remote(selected_source).unwrap());
            session.release_media_track_reference(selected).unwrap();
            session.release_track_source(selected_source).unwrap();
            session.shutdown().unwrap();
            assert!(!session.dtmf_can_insert_tones(dtmf).unwrap());
            session.release_dtmf_sender(dtmf).unwrap();
        })
        .join()
        .unwrap();
    }
}
