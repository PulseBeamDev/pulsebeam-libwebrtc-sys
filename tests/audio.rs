use std::{net::Ipv4Addr, time::Duration};

use pulsebeam_webrtc_sys::{
    AudioFrameError, AudioPcmFrame, Environment, ManualClock, PeerConfiguration,
    PeerConnectionFactory, PeerErrorKind, RtpTransceiverDirection, SimulatedNetwork,
};

#[test]
fn caller_fed_audio_track_rejects_foreign_sources_and_survives_source_owner_drop() {
    let clock = ManualClock::new(Duration::from_secs(1)).unwrap();
    let environment = Environment::builder().clock(&clock).build().unwrap();
    let network = SimulatedNetwork::new(&clock).unwrap();
    let endpoint_a = network
        .register_endpoint(Ipv4Addr::new(10, 6, 0, 1).into())
        .unwrap();
    let endpoint_b = network
        .register_endpoint(Ipv4Addr::new(10, 6, 0, 2).into())
        .unwrap();
    let build = |endpoint: &pulsebeam_webrtc_sys::NetworkEndpoint| {
        PeerConnectionFactory::builder()
            .environment(environment.clone())
            .network_manager(endpoint.network_manager().unwrap())
            .packet_socket_factory(endpoint.packet_socket_factory().unwrap())
            .build()
            .unwrap()
    };
    let factory = build(&endpoint_a);
    let other = build(&endpoint_b);
    let mut source = factory.create_audio_source().unwrap();
    assert_eq!(
        other
            .create_audio_track("foreign", &source)
            .err()
            .unwrap()
            .kind,
        PeerErrorKind::InvalidParameter
    );
    assert_eq!(
        factory.create_audio_track("", &source).err().unwrap().kind,
        PeerErrorKind::InvalidParameter
    );
    let frame = AudioPcmFrame::i16_interleaved(48_000, 1, 1_000_000, vec![0; 480]).unwrap();
    source.push_frame(&frame).unwrap();
    let mut malformed = frame.clone();
    malformed.samples.pop();
    assert_eq!(
        source.push_frame(&malformed),
        Err(AudioFrameError::InvalidSampleCount)
    );
    let track = factory.create_audio_track("mic", &source).unwrap();
    assert_eq!(track.id(), "mic");
    assert!(track.enabled());
    track.set_enabled(false);
    assert!(!track.enabled());
    track.set_enabled(true);

    let mut peer = factory
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    assert_eq!(
        other
            .create_peer_connection(PeerConfiguration::default())
            .unwrap()
            .add_audio_transceiver(&track, RtpTransceiverDirection::SendOnly)
            .err()
            .unwrap()
            .kind,
        PeerErrorKind::InvalidParameter
    );
    let transceiver = peer
        .add_audio_transceiver(&track, RtpTransceiverDirection::SendOnly)
        .unwrap();
    assert!(!transceiver.sender().id().is_empty());
    source.push_frame(&frame).unwrap();
    source.close().unwrap();
    assert_eq!(source.push_frame(&frame), Err(AudioFrameError::Released));
    drop(source);
    peer.remove_track(&transceiver.sender()).unwrap();
    peer.close().unwrap();
    drop(track);
    drop(peer);
    drop(factory);
}
