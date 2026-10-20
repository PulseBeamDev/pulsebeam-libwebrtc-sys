//! Stock OpenH264/FFmpeg native media at 720p.
#[path = "support/managed_video.rs"]
mod managed_video;

#[test]
fn high_resolution_stock_h264_uses_managed_call_defaults_and_replays() {
    managed_video::check_replay(
        "H264",
        &[("packetization-mode", "1"), ("profile-level-id", "42e01f")],
        "PULSEBEAM_H264_HOST_WORKER_PROBE",
        "PULSEBEAM_H264_TRACE_PATH",
    );
}
