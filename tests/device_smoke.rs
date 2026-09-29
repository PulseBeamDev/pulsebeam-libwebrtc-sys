use pulsebeam_webrtc_sys::{PeerConnectionFactory, PeerErrorKind};

#[test]
fn native_audio_opt_in_is_a_device_independent_smoke() {
    // Core rejects the opt-in. Native can still be unavailable on a CI host
    // without recording/playout hardware. Never require a physical device.
    match PeerConnectionFactory::builder().native_audio(true).build() {
        Ok(factory) => {
            for recording in [true, false] {
                match factory.audio_devices(recording) {
                    Ok(devices) => {
                        for device in devices {
                            assert!(!device.name.is_empty());
                        }
                    }
                    Err(error) => assert!(!error.message.is_empty()),
                }
            }
            assert!(factory.select_audio_device(true, u16::MAX).is_err());
        }
        Err(error) => {
            assert_eq!(error.kind, PeerErrorKind::NativeConstruction);
            assert!(!error.message.is_empty());
        }
    }
}

#[test]
fn headless_factory_explicitly_rejects_platform_device_calls() {
    let factory = PeerConnectionFactory::builder().build().unwrap();
    assert_eq!(
        factory.audio_devices(true).unwrap_err().kind,
        PeerErrorKind::UnsupportedOperation
    );
    assert_eq!(
        factory.create_microphone_track("mic").err().unwrap().kind,
        PeerErrorKind::UnsupportedOperation
    );
}
