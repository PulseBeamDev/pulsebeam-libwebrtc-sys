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
        assert!(encoder.supported_formats().iter().any(|format| format.name == "H264"));
        assert!(decoder.supported_formats().iter().any(|format| format.name == "H264"));
        assert!(encoder.query_support(&format, None, None).supported);
        assert!(decoder.query_support(&format, false, None).supported);
        let factory = pulsebeam_webrtc_sys::PeerConnectionFactory::builder()
            .environment(environment)
            .video_encoder_factory(encoder)
            .video_decoder_factory(decoder)
            .build()
            .unwrap();
        let peer = factory
            .create_peer_connection(pulsebeam_webrtc_sys::PeerConfiguration::default())
            .unwrap();
        assert!(peer.video_sender_capabilities().unwrap().iter()
            .any(|codec| codec.format().name == "H264"));
        peer.close().unwrap();
    }
}
