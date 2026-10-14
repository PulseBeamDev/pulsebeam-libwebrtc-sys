use std::time::Duration;

fn main() {
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
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let encoder = pulsebeam_webrtc_sys::VideoEncoderFactoryHandle::builtin().unwrap();
        let decoder = pulsebeam_webrtc_sys::VideoDecoderFactoryHandle::builtin().unwrap();
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
        let factory = pulsebeam_webrtc_sys::PeerConnectionFactory::builder()
            .environment(environment)
            .video_encoder_factory(encoder)
            .video_decoder_factory(decoder)
            .build()
            .unwrap();
        let mut peer = factory
            .create_peer_connection(pulsebeam_webrtc_sys::PeerConfiguration::default())
            .unwrap();
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
        let dtmf = session.sender_dtmf(sender).unwrap().unwrap();
        assert_eq!(session.sender_dtmf(sender).unwrap(), Some(dtmf));
        assert!(!session.dtmf_can_insert_tones(dtmf).unwrap());
        assert!(
            session
                .register_dtmf_observer(dtmf, std::num::NonZeroUsize::new(2).unwrap())
                .unwrap()
        );
        std::thread::spawn(move || {
            session.shutdown().unwrap();
            assert!(!session.dtmf_can_insert_tones(dtmf).unwrap());
            session.release_dtmf_sender(dtmf).unwrap();
        })
        .join()
        .unwrap();
    }
}
