use pulsebeam_webrtc_sys::*;

#[test]
fn native_option_values_preserve_all_fields_presence_and_binary_configuration() {
    let changes = AudioSourceOptions {
        echo_cancellation: Some(false),
        echo_cancellation_mode: Some(AudioProcessingMode::Automatic),
        auto_gain_control: Some(true),
        auto_gain_control_mode: Some(AudioProcessingMode::Platform),
        noise_suppression: Some(false),
        noise_suppression_mode: Some(AudioProcessingMode::Software),
        highpass_filter: Some(true),
        highpass_filter_mode: Some(AudioProcessingMode::Unknown(-73)),
        stereo_swapping: Some(false),
        // Owning values preserve the full integer domain. Applying them to
        // an engine is a separate operation with its own consumption rules.
        audio_jitter_buffer_max_packets: Some(i32::MAX),
        audio_jitter_buffer_fast_accelerate: Some(false),
        audio_jitter_buffer_min_delay_ms: Some(i32::MIN),
        audio_network_adaptor: Some(false),
        audio_network_adaptor_config: Some(vec![0, 0xff, b'x', 0, 0x80]),
        init_recording_on_send: Some(false),
    };
    let mut actual = AudioSourceOptions::default();
    assert_ne!(actual, changes);
    actual.set_all(&changes);
    // Compare the reified Rust fields too, so dropping a field on both sides
    // of native operator== cannot hide an incomplete bridge conversion.
    assert_eq!(format!("{actual:?}"), format!("{changes:?}"));
    assert_eq!(actual, changes);
    actual.set_all(&AudioSourceOptions::default());
    assert_eq!(format!("{actual:?}"), format!("{changes:?}"));
    let description = actual.native_description();
    assert!(description.contains("aec: 0"));
    assert!(description.contains("agc_mode: platform"));
    assert!(description.contains("ns_mode: software"));
    assert!(!description.contains("adaptor_config"));

    let mut empty_config = AudioSourceOptions::default();
    empty_config.audio_network_adaptor_config = Some(vec![]);
    assert_ne!(empty_config, AudioSourceOptions::default());
    let mut copied = AudioSourceOptions::default();
    copied.set_all(&empty_config);
    assert_eq!(copied.audio_network_adaptor_config, Some(vec![]));
    let mut explicit = AudioSourceOptions::default();
    explicit.audio_jitter_buffer_min_delay_ms = Some(0);
    explicit.echo_cancellation = Some(false);
    copied.set_all(&explicit);
    assert_eq!(copied.audio_jitter_buffer_min_delay_ms, Some(0));
    assert_eq!(copied.echo_cancellation, Some(false));
    assert_eq!(copied.audio_network_adaptor_config, Some(vec![]));
}

#[test]
fn source_and_installed_track_queries_follow_native_state_and_detachment() {
    let factory = PeerConnectionFactory::builder().build().unwrap();
    let mut source = factory.create_audio_source().unwrap();
    let track = factory
        .create_audio_track("options-track", &source)
        .unwrap();
    assert_eq!(source.state(), AudioSourceState::Live);
    assert!(!source.is_remote());
    assert_eq!(source.volume(), 1.0);
    assert_eq!(source.options(), AudioSourceOptions::default());
    assert_eq!(track.kind(), "audio");
    assert_eq!(track.state(), AudioTrackState::Live);
    assert_eq!(track.signal_level(), None); // Pinned native default, not zero.
    assert_eq!(track.volume(), 1.0);
    track
        .set_audio_processing_options(AudioProcessingOptions {
            echo_cancellation: ProcessingChoice::Software,
            noise_suppression: ProcessingChoice::Disabled,
            gain_control: ProcessingChoice::Automatic,
        })
        .unwrap();
    let options = source.options();
    assert_eq!(options.echo_cancellation, Some(true));
    assert_eq!(
        options.echo_cancellation_mode,
        Some(AudioProcessingMode::Software)
    );
    assert_eq!(options.noise_suppression, Some(false));
    assert_eq!(options.noise_suppression_mode, None);
    assert_eq!(options.auto_gain_control, Some(true));
    assert_eq!(
        options.auto_gain_control_mode,
        Some(AudioProcessingMode::Automatic)
    );
    assert_eq!(options.highpass_filter, None);
    let mut peer = factory
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    let transceiver = peer
        .add_audio_transceiver(&track, RtpTransceiverDirection::SendOnly)
        .unwrap();
    let sender = transceiver.sender();
    let before = sender.audio_track_status().unwrap();
    assert_eq!(before, track.status());
    assert_eq!(before.id, "options-track");
    assert_eq!(before.kind, "audio");
    assert!(before.enabled);
    assert!(track.set_enabled(false));
    assert!(!sender.audio_track_status().unwrap().enabled);
    sender.set_audio_track(None).unwrap();
    assert_eq!(sender.audio_track_status(), None);
    sender.set_audio_track(Some(&track)).unwrap();
    assert_eq!(sender.audio_track_status().unwrap().id, before.id);
    source.close().unwrap();
    assert_eq!(source.state(), AudioSourceState::Ended);
    assert_eq!(track.state(), AudioTrackState::Ended);
    assert_eq!(
        sender.audio_track_status().unwrap().state,
        AudioTrackState::Ended
    );
    assert_eq!(source.options(), options);
    peer.close().unwrap();
    drop((peer, transceiver, sender, track, source, factory));
    assert_eq!(before.state, AudioTrackState::Live);
    assert!(before.enabled);
    assert_eq!(options.echo_cancellation, Some(true));
}
