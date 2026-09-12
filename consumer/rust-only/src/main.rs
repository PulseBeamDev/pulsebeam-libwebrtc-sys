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
}
