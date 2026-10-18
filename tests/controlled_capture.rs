#![cfg(feature = "native")]

use pulsebeam_webrtc_sys::{
    CameraControlError, CameraSettings, ControlledWorld, PeerError, PeerErrorKind,
};
use std::{net::Ipv4Addr, time::Duration};

fn rejected<T>(result: Result<T, PeerError>) {
    let error = result
        .err()
        .expect("controlled execution admitted physical capture");
    assert_eq!(error.kind, PeerErrorKind::UnsupportedOperation);
    assert_eq!(
        error.message,
        "physical video capture is unavailable in controlled execution"
    );
}

fn rejected_control<T>(result: Result<T, CameraControlError>) {
    match result {
        Err(CameraControlError::Binding(error)) => rejected::<()>(Err(error)),
        _ => panic!("controlled camera control entered platform code"),
    }
}

#[test]
fn controlled_capture_rejects_before_platform_discovery_or_source_creation() {
    let world = ControlledWorld::acquire(404, Duration::from_secs(10)).unwrap();
    let network = world.create_network().unwrap();
    let endpoint = network
        .register_endpoint(Ipv4Addr::new(10, 4, 0, 1).into())
        .unwrap();
    let factory = world
        .peer_factory_builder()
        .unwrap()
        .network_manager(endpoint.network_manager().unwrap())
        .packet_socket_factory(endpoint.packet_socket_factory().unwrap())
        .build()
        .unwrap();
    let before = (world.now(), world.next_deadline());

    rejected(factory.create_camera_options());
    rejected(factory.create_camera_device_info());
    rejected(factory.create_camera("camera"));
    rejected(factory.desktop_capture_options(false, true));
    rejected(factory.desktop_capture_options(true, false));
    rejected_control(factory.camera_capabilities("camera"));
    rejected_control(factory.camera_orientation("camera"));
    rejected_control(factory.best_camera_capability("camera", &CameraSettings::default()));
    rejected(factory.camera_devices());
    rejected(factory.camera_formats("camera"));
    rejected(factory.open_camera("camera", 640, 480, 30));
    rejected(factory.screen_sources());
    rejected(factory.window_sources());
    rejected(factory.open_screen(0));
    rejected(factory.open_window(0));
    // Rejection must not depend on discovering hardware or validating a
    // selection. Even an invalid selection must never enter platform code.
    rejected(factory.camera_formats(""));
    rejected(factory.open_camera("", 0, 0, 0));
    rejected(factory.open_screen(i64::MIN));
    rejected(factory.open_window(i64::MIN));

    assert_eq!((world.now(), world.next_deadline()), before);
    assert!(network.next_packet().is_none());
}
