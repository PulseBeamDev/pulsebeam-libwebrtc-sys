fn main() {
    assert_eq!(
        pulsebeam_webrtc_sys::bridge_identity(),
        "pulsebeam-webrtc-sys-bridge-v1"
    );
}
