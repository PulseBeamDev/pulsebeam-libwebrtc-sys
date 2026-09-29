//! Headless, public-API-only setup for raw PCM/I420 and externally encoded H.264.
//!
//! This stages local tracks; it does not negotiate a remote peer or prove delivery.
//! For negotiated send/receive, see tests/audio.rs and tests/video.rs.
//! `cargo run --example core_media -- path/to/annex-b-idr.h264` stages a
//! caller-owned keyframe without an H.264 encoder. The file must contain a
//! complete Annex-B access unit matching constrained-baseline level 3.1.
use pulsebeam_webrtc_sys::{
    AudioPcmFrame, EncodedH264Input, H264AccessUnit, PeerConfiguration, PeerConnectionFactory,
    RtpTransceiverDirection, VideoFrame,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let raw_factory = PeerConnectionFactory::builder().build()?;
    let audio = raw_factory.create_audio_source()?;
    let audio_track = raw_factory.create_audio_track("pcm", &audio)?;
    let video = raw_factory.create_video_source()?;
    let video_track = raw_factory.create_video_track("i420", &video)?;
    let mut raw_peer = raw_factory.create_peer_connection(PeerConfiguration::default())?;
    raw_peer.add_audio_transceiver(&audio_track, RtpTransceiverDirection::SendOnly)?;
    raw_peer.add_video_transceiver(&video_track, RtpTransceiverDirection::SendOnly)?;
    audio.push_frame(&AudioPcmFrame::i16_interleaved(48_000, 1, 0, vec![0; 480])?)?;
    video.push_frame(&VideoFrame::i420(
        2,
        2,
        vec![16, 16, 16, 16, 128, 128],
        0,
        0,
    )?)?;

    if let Some(path) = std::env::args_os().nth(1) {
        let input = EncodedH264Input::new()?;
        let encoded_factory = PeerConnectionFactory::builder()
            .video_encoder_factory(input.encoder_factory())
            .build()?;
        let source = input.create_source(&encoded_factory)?;
        let track = source.create_track(&encoded_factory, "external-h264")?;
        let mut encoded_peer =
            encoded_factory.create_peer_connection(PeerConfiguration::default())?;
        encoded_peer.add_video_transceiver(&track, RtpTransceiverDirection::SendOnly)?;
        source.push(H264AccessUnit {
            data: std::fs::read(path)?,
            width: 16,
            height: 16,
            timestamp_us: 0,
            key_frame: true,
            qp: None,
        })?;
        println!("staged encoded stream {}", source.stream_id());
        encoded_peer.close()?;
    }
    raw_peer.close()?;
    Ok(())
}
