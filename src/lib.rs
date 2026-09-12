//! Low-level Rust integration with PulseBeam's pinned libwebrtc artifact.

mod codec;
mod data_channel;
mod execution;
mod network;
mod peer;
mod video;

pub(crate) use codec::{
    RustVideoDecoder, RustVideoDecoderFactory, RustVideoEncoder, RustVideoEncoderFactory,
    decoder_configure, decoder_decode, decoder_factory_create, decoder_factory_formats,
    decoder_factory_query, decoder_get_info, decoder_is_valid, decoder_register_callback,
    decoder_release, encoder_encode, encoder_factory_create, encoder_factory_formats,
    encoder_factory_query, encoder_get_info, encoder_init, encoder_is_valid,
    encoder_register_callback, encoder_release, encoder_set_rates,
};
pub(crate) use execution::{RustTask, run_task};

pub use codec::{
    AudioDecoderFactory, AudioEncoderFactory, CodecError, CodecParameter, CodecSupport,
    DecodedImageCallback, EncodedImageCallback, EncodedVideoFrame, VideoCodecFormat, VideoDecoder,
    VideoDecoderFactory, VideoDecoderFactoryHandle, VideoDecoderInfo, VideoDecoderSettings,
    VideoEncoder, VideoEncoderFactory, VideoEncoderFactoryHandle, VideoEncoderInfo,
    VideoEncoderSettings, VideoFrame, VideoFrameBuffer, VideoFrameType, VideoRateControl,
    VideoResolution,
};
pub use data_channel::{
    DataChannel, DataChannelConfiguration, DataChannelEvent, DataChannelMessage,
    DataChannelMessageKind, DataChannelPriority, DataChannelSendResult, DataChannelState,
};
pub use execution::{
    BuildEnvironmentError, Environment, EnvironmentBuilder, ManualClock, NetworkThread,
    QueuePriority, RandomnessLease, RandomnessLeaseError, SignalingThread, SystemClock, TaskQueue,
    TaskQueueFactory, ThreadStartError, WorkerThread,
};
pub use network::{
    NetworkAddress, NetworkEndpoint, NetworkError, NetworkManagerProvider, OutboundPacket,
    PacketSocketFactoryProvider, ReceivedPacket, SimulatedNetwork, SimulatedUdpSocket,
};
pub use peer::{
    ConnectionState, IceCandidate, IceGatheringState, OperationCompletion, OperationId,
    PeerConfiguration, PeerConnection, PeerConnectionEvent, PeerConnectionFactory,
    PeerConnectionFactoryBuilder, PeerError, PeerErrorKind, SessionDescription,
    SessionDescriptionType, SignalingState,
};
pub use video::{
    RtpReceiver, RtpSender, RtpTransceiver, RtpTransceiverDirection, VideoSink, VideoSource,
    VideoSourceState, VideoTrack, VideoTrackState,
};

#[cxx::bridge(namespace = "pulsebeam::webrtc_sys")]
mod ffi {
    struct FfiCodecParameter {
        key: String,
        value: String,
    }

    struct FfiCodecFormat {
        name: String,
        parameters: Vec<FfiCodecParameter>,
    }

    struct FfiCodecSupport {
        supported: bool,
        power_efficient: bool,
    }

    struct FfiEncoderSettings {
        width: u32,
        height: u32,
        start_bitrate_bps: u32,
        max_bitrate_bps: u32,
        min_bitrate_bps: u32,
        max_framerate: u32,
        cores: u32,
        max_payload_size: u32,
    }

    struct FfiDecoderSettings {
        cores: u32,
        max_width: u32,
        max_height: u32,
    }

    struct FfiRateControl {
        bitrate_bps: u32,
        framerate_fps: f64,
        bandwidth_bps: u64,
    }

    struct FfiEncoderInfo {
        implementation_name: String,
        hardware_accelerated: bool,
        supports_native_handle: bool,
    }

    struct FfiDecoderInfo {
        implementation_name: String,
        hardware_accelerated: bool,
    }

    struct FfiPeerEvent {
        kind: u8,
        operation_id: u64,
        has_description: bool,
        sdp_type: u8,
        state: u8,
        event_id: u32,
        sdp_mline_index: i32,
        port: i32,
        error_code: i32,
        error_type: u8,
        sdp: String,
        sdp_mid: String,
        candidate: String,
        address: String,
        url: String,
        message: String,
    }

    struct FfiDataChannelEvent {
        kind: u8,
        state: u8,
        binary: bool,
        sent_data_size: u64,
        data: Vec<u8>,
    }

    #[allow(dead_code)]
    struct FfiCodecTestResult {
        status: i32,
        encoded_frames: u32,
        decoded_frames: u32,
        checksum: u64,
    }

    extern "Rust" {
        type RustTask;
        type RustVideoEncoderFactory;
        type RustVideoDecoderFactory;
        type RustVideoEncoder;
        type RustVideoDecoder;

        fn run_task(task: Box<RustTask>);

        fn encoder_factory_formats(factory: &RustVideoEncoderFactory) -> Vec<FfiCodecFormat>;
        fn encoder_factory_query(
            factory: &RustVideoEncoderFactory,
            format: &FfiCodecFormat,
            scalability_mode: &str,
            has_resolution: bool,
            width: u32,
            height: u32,
        ) -> FfiCodecSupport;
        fn encoder_factory_create(
            factory: &RustVideoEncoderFactory,
            format: &FfiCodecFormat,
        ) -> Box<RustVideoEncoder>;
        fn encoder_is_valid(encoder: &RustVideoEncoder) -> bool;
        fn encoder_init(encoder: &mut RustVideoEncoder, settings: FfiEncoderSettings) -> i32;
        fn encoder_register_callback(encoder: &mut RustVideoEncoder) -> i32;
        fn encoder_encode(
            encoder: &mut RustVideoEncoder,
            frame: UniquePtr<NativeVideoFrame>,
            frame_types: &[u8],
            callback: SharedPtr<NativeEncodedImageCallback>,
        ) -> i32;
        fn encoder_set_rates(encoder: &mut RustVideoEncoder, rates: FfiRateControl) -> i32;
        fn encoder_release(encoder: &mut RustVideoEncoder) -> i32;
        fn encoder_get_info(encoder: &RustVideoEncoder) -> FfiEncoderInfo;

        fn decoder_factory_formats(factory: &RustVideoDecoderFactory) -> Vec<FfiCodecFormat>;
        fn decoder_factory_query(
            factory: &RustVideoDecoderFactory,
            format: &FfiCodecFormat,
            reference_scaling: bool,
            has_resolution: bool,
            width: u32,
            height: u32,
        ) -> FfiCodecSupport;
        fn decoder_factory_create(
            factory: &RustVideoDecoderFactory,
            format: &FfiCodecFormat,
        ) -> Box<RustVideoDecoder>;
        fn decoder_is_valid(decoder: &RustVideoDecoder) -> bool;
        fn decoder_configure(decoder: &mut RustVideoDecoder, settings: FfiDecoderSettings) -> bool;
        fn decoder_register_callback(decoder: &mut RustVideoDecoder) -> i32;
        fn decoder_decode(
            decoder: &mut RustVideoDecoder,
            frame: UniquePtr<NativeEncodedVideoFrame>,
            callback: SharedPtr<NativeDecodedImageCallback>,
        ) -> i32;
        fn decoder_release(decoder: &mut RustVideoDecoder) -> i32;
        fn decoder_get_info(decoder: &RustVideoDecoder) -> FfiDecoderInfo;
    }

    unsafe extern "C++" {
        include!("pulsebeam-webrtc-sys/native/probe.h");
        include!("pulsebeam-webrtc-sys/native/execution.h");
        include!("pulsebeam-webrtc-sys/native/network.h");
        include!("pulsebeam-webrtc-sys/native/codec.h");
        include!("pulsebeam-webrtc-sys/native/peer.h");
        include!("pulsebeam-webrtc-sys/native/data_channel.h");
        include!("pulsebeam-webrtc-sys/native/video.h");

        type NativeEnvironment;
        type NativeManualClock;
        type NativeRandomnessLease;
        type NativeTaskQueue;
        type NativeTaskQueueFactory;
        type NativeThread;
        type NativeSimulatedNetwork;
        type NativeNetworkEndpoint;
        type NativeNetworkManagerProvider;
        type NativePacketSocketFactoryProvider;
        type NativeSimulatedUdpSocket;
        type NativeOutboundPacket;
        type NativeReceivedPacket;
        type NativeVideoEncoderFactory;
        type NativeVideoDecoderFactory;
        type NativeAudioEncoderFactory;
        type NativeAudioDecoderFactory;
        type NativeVideoFrame;
        type NativeEncodedVideoFrame;
        type NativeEncodedImageCallback;
        type NativeDecodedImageCallback;
        type NativePeerConnectionFactory;
        type NativePeerConnection;
        type NativeDataChannel;
        type NativeVideoSource;
        type NativeVideoTrack;
        type NativeVideoSink;
        type NativeRtpSender;
        type NativeRtpReceiver;
        type NativeRtpTransceiver;

        fn bridge_identity() -> &'static str;

        fn new_manual_clock(initial_time_us: i64) -> UniquePtr<NativeManualClock>;
        fn manual_clock_time_us(clock: &NativeManualClock) -> i64;
        fn advance_manual_clock(clock: &NativeManualClock, delta_us: i64) -> bool;
        fn system_clock_time_us() -> i64;

        fn new_default_task_queue_factory() -> UniquePtr<NativeTaskQueueFactory>;
        fn new_cooperative_task_queue_factory(
            clock: &NativeManualClock,
        ) -> UniquePtr<NativeTaskQueueFactory>;
        fn task_queue_factory_is_cooperative(factory: &NativeTaskQueueFactory) -> bool;
        fn task_queue_factory_uses_clock(
            factory: &NativeTaskQueueFactory,
            clock: &NativeManualClock,
        ) -> bool;
        fn create_task_queue(
            factory: &NativeTaskQueueFactory,
            name: &str,
            priority: u8,
        ) -> UniquePtr<NativeTaskQueue>;
        fn post_task(queue: &NativeTaskQueue, task: Box<RustTask>) -> bool;
        fn post_delayed_task(queue: &NativeTaskQueue, delay_us: i64, task: Box<RustTask>) -> bool;
        fn run_ready_tasks(factory: &NativeTaskQueueFactory) -> usize;
        fn next_task_deadline_us(factory: &NativeTaskQueueFactory) -> i64;

        unsafe fn create_environment(
            clock: *const NativeManualClock,
            factory: &NativeTaskQueueFactory,
        ) -> UniquePtr<NativeEnvironment>;
        fn clone_environment(environment: &NativeEnvironment) -> UniquePtr<NativeEnvironment>;
        fn environment_time_us(environment: &NativeEnvironment) -> i64;

        fn new_seeded_randomness(seed: u64) -> UniquePtr<NativeRandomnessLease>;
        fn next_seeded_random_u64(lease: &NativeRandomnessLease) -> u64;

        fn new_thread(network: bool) -> UniquePtr<NativeThread>;
        fn thread_post_task(thread: &NativeThread, task: Box<RustTask>) -> bool;
        fn thread_post_delayed_task(
            thread: &NativeThread,
            delay_us: i64,
            task: Box<RustTask>,
        ) -> bool;

        fn new_simulated_network(clock: &NativeManualClock) -> UniquePtr<NativeSimulatedNetwork>;
        fn register_network_endpoint(
            network: &NativeSimulatedNetwork,
            ip: &[u8],
            error: &mut u8,
        ) -> UniquePtr<NativeNetworkEndpoint>;
        fn close_network_endpoint(endpoint: Pin<&mut NativeNetworkEndpoint>);
        fn new_network_manager_provider(
            endpoint: &NativeNetworkEndpoint,
        ) -> UniquePtr<NativeNetworkManagerProvider>;
        fn network_manager_provider_is_valid(provider: &NativeNetworkManagerProvider) -> bool;
        fn new_packet_socket_factory_provider(
            endpoint: &NativeNetworkEndpoint,
        ) -> UniquePtr<NativePacketSocketFactoryProvider>;
        fn packet_socket_factory_supports_udp(provider: &NativePacketSocketFactoryProvider)
        -> bool;
        fn packet_socket_factory_supports_tcp(provider: &NativePacketSocketFactoryProvider)
        -> bool;
        fn packet_socket_factory_supports_dns(provider: &NativePacketSocketFactoryProvider)
        -> bool;
        fn create_simulated_udp_socket(
            provider: &NativePacketSocketFactoryProvider,
            port: u16,
            error: &mut u8,
        ) -> UniquePtr<NativeSimulatedUdpSocket>;
        fn simulated_udp_local_ip(socket: &NativeSimulatedUdpSocket) -> Vec<u8>;
        fn simulated_udp_local_port(socket: &NativeSimulatedUdpSocket) -> u16;
        fn simulated_udp_send_to(
            socket: &NativeSimulatedUdpSocket,
            destination_ip: &[u8],
            destination_port: u16,
            payload: Vec<u8>,
            error: &mut u8,
        ) -> bool;
        fn simulated_udp_take_received(
            socket: &NativeSimulatedUdpSocket,
        ) -> UniquePtr<NativeReceivedPacket>;
        fn close_simulated_udp_socket(socket: Pin<&mut NativeSimulatedUdpSocket>);
        fn take_outbound_packet(
            network: &NativeSimulatedNetwork,
        ) -> UniquePtr<NativeOutboundPacket>;
        fn outbound_packet_id(packet: &NativeOutboundPacket) -> u64;
        fn outbound_packet_source_ip(packet: &NativeOutboundPacket) -> Vec<u8>;
        fn outbound_packet_source_port(packet: &NativeOutboundPacket) -> u16;
        fn outbound_packet_destination_ip(packet: &NativeOutboundPacket) -> Vec<u8>;
        fn outbound_packet_destination_port(packet: &NativeOutboundPacket) -> u16;
        fn outbound_packet_payload(packet: &NativeOutboundPacket) -> Vec<u8>;
        fn outbound_packet_deadline_us(packet: &NativeOutboundPacket) -> i64;
        fn deliver_outbound_packet(
            network: &NativeSimulatedNetwork,
            packet_id: u64,
            keep_pending: bool,
            error: &mut u8,
        ) -> bool;
        fn drop_outbound_packet(
            network: &NativeSimulatedNetwork,
            packet_id: u64,
            error: &mut u8,
        ) -> bool;
        fn received_packet_source_ip(packet: &NativeReceivedPacket) -> Vec<u8>;
        fn received_packet_source_port(packet: &NativeReceivedPacket) -> u16;
        fn received_packet_payload(packet: &NativeReceivedPacket) -> Vec<u8>;

        fn new_video_encoder_factory(
            factory: Box<RustVideoEncoderFactory>,
        ) -> UniquePtr<NativeVideoEncoderFactory>;
        fn new_video_decoder_factory(
            factory: Box<RustVideoDecoderFactory>,
        ) -> UniquePtr<NativeVideoDecoderFactory>;
        fn new_builtin_audio_encoder_factory() -> UniquePtr<NativeAudioEncoderFactory>;
        fn new_builtin_audio_decoder_factory() -> UniquePtr<NativeAudioDecoderFactory>;
        fn video_encoder_formats(factory: &NativeVideoEncoderFactory) -> Vec<FfiCodecFormat>;
        fn video_encoder_query(
            factory: &NativeVideoEncoderFactory,
            format: &FfiCodecFormat,
            scalability_mode: &str,
            has_resolution: bool,
            width: u32,
            height: u32,
        ) -> FfiCodecSupport;
        fn video_decoder_formats(factory: &NativeVideoDecoderFactory) -> Vec<FfiCodecFormat>;
        fn video_decoder_query(
            factory: &NativeVideoDecoderFactory,
            format: &FfiCodecFormat,
            reference_scaling: bool,
            has_resolution: bool,
            width: u32,
            height: u32,
        ) -> FfiCodecSupport;
        fn native_video_frame_width(frame: &NativeVideoFrame) -> u32;
        fn native_video_frame_height(frame: &NativeVideoFrame) -> u32;
        fn native_video_frame_timestamp_us(frame: &NativeVideoFrame) -> i64;
        fn native_video_frame_rtp_timestamp(frame: &NativeVideoFrame) -> u32;
        fn native_video_frame_i420(frame: &NativeVideoFrame) -> Vec<u8>;
        fn native_encoded_frame_width(frame: &NativeEncodedVideoFrame) -> u32;
        fn native_encoded_frame_height(frame: &NativeEncodedVideoFrame) -> u32;
        fn native_encoded_frame_rtp_timestamp(frame: &NativeEncodedVideoFrame) -> u32;
        fn native_encoded_frame_key(frame: &NativeEncodedVideoFrame) -> bool;
        fn native_encoded_frame_qp(frame: &NativeEncodedVideoFrame) -> i32;
        fn native_encoded_frame_data(frame: &NativeEncodedVideoFrame) -> Vec<u8>;
        fn encoded_callback_emit(
            callback: &NativeEncodedImageCallback,
            data: &[u8],
            width: u32,
            height: u32,
            rtp_timestamp: u32,
            key_frame: bool,
            qp: i32,
        ) -> bool;
        fn decoded_callback_emit(
            callback: &NativeDecodedImageCallback,
            data: &[u8],
            width: u32,
            height: u32,
            timestamp_us: i64,
            rtp_timestamp: u32,
        ) -> bool;
        #[allow(dead_code)]
        fn test_codec_roundtrip(
            encoder: &NativeVideoEncoderFactory,
            decoder: &NativeVideoDecoderFactory,
            frames: u32,
        ) -> FfiCodecTestResult;
        #[allow(dead_code)]
        fn test_encoder_factory_cross_thread(factory: &NativeVideoEncoderFactory) -> bool;

        unsafe fn new_peer_connection_factory(
            environment: &NativeEnvironment,
            network_thread: &NativeThread,
            worker_thread: &NativeThread,
            signaling_thread: &NativeThread,
            network_manager: *const NativeNetworkManagerProvider,
            packet_socket_factory: *const NativePacketSocketFactoryProvider,
            audio_encoder: *const NativeAudioEncoderFactory,
            audio_decoder: *const NativeAudioDecoderFactory,
            video_encoder: *const NativeVideoEncoderFactory,
            video_decoder: *const NativeVideoDecoderFactory,
            error: &mut String,
        ) -> UniquePtr<NativePeerConnectionFactory>;
        fn create_peer_connection(
            factory: &NativePeerConnectionFactory,
            ice_candidate_pool_size: u16,
            always_negotiate_data_channels: bool,
            error: &mut String,
        ) -> UniquePtr<NativePeerConnection>;
        fn peer_create_offer(peer: &NativePeerConnection, operation_id: u64);
        fn peer_create_answer(peer: &NativePeerConnection, operation_id: u64);
        fn peer_set_local_description(
            peer: &NativePeerConnection,
            operation_id: u64,
            sdp_type: u8,
            sdp: &str,
        );
        fn peer_set_remote_description(
            peer: &NativePeerConnection,
            operation_id: u64,
            sdp_type: u8,
            sdp: &str,
        );
        fn peer_add_ice_candidate(
            peer: &NativePeerConnection,
            operation_id: u64,
            sdp_mid: &str,
            sdp_mline_index: i32,
            candidate: &str,
        );
        fn peer_take_event(peer: &NativePeerConnection) -> FfiPeerEvent;
        fn peer_take_data_channel(
            peer: &NativePeerConnection,
            arrival_id: u64,
        ) -> UniquePtr<NativeDataChannel>;
        fn close_peer_connection(peer: &NativePeerConnection) -> bool;

        fn create_data_channel(
            peer: &NativePeerConnection,
            label: &str,
            ordered: bool,
            max_retransmit_time_ms: i32,
            max_retransmits: i32,
            protocol: &str,
            negotiated: bool,
            id: i32,
            priority: i8,
            error_type: &mut u8,
            error: &mut String,
        ) -> UniquePtr<NativeDataChannel>;
        fn data_channel_label(channel: &NativeDataChannel) -> String;
        fn data_channel_ordered(channel: &NativeDataChannel) -> bool;
        fn data_channel_max_retransmit_time_ms(channel: &NativeDataChannel) -> i32;
        fn data_channel_max_retransmits(channel: &NativeDataChannel) -> i32;
        fn data_channel_protocol(channel: &NativeDataChannel) -> String;
        fn data_channel_negotiated(channel: &NativeDataChannel) -> bool;
        fn data_channel_id(channel: &NativeDataChannel) -> i32;
        fn data_channel_priority(channel: &NativeDataChannel) -> u8;
        fn data_channel_state(channel: &NativeDataChannel) -> u8;
        fn data_channel_buffered_amount(channel: &NativeDataChannel) -> u64;
        fn data_channel_send(channel: &NativeDataChannel, data: &[u8], binary: bool) -> u8;
        fn data_channel_take_event(channel: &NativeDataChannel) -> FfiDataChannelEvent;
        fn close_data_channel(channel: &NativeDataChannel) -> bool;

        fn create_video_source(
            factory: &NativePeerConnectionFactory,
        ) -> UniquePtr<NativeVideoSource>;
        fn close_video_source(source: &NativeVideoSource) -> bool;
        fn video_source_state(source: &NativeVideoSource) -> u8;
        fn video_source_push_frame(
            source: &NativeVideoSource,
            data: &[u8],
            width: u32,
            height: u32,
            timestamp_us: i64,
            rtp_timestamp: u32,
        ) -> bool;
        fn create_video_track(
            factory: &NativePeerConnectionFactory,
            source: &NativeVideoSource,
            id: &str,
        ) -> UniquePtr<NativeVideoTrack>;
        fn video_track_id(track: &NativeVideoTrack) -> String;
        fn video_track_enabled(track: &NativeVideoTrack) -> bool;
        fn video_track_set_enabled(track: &NativeVideoTrack, enabled: bool) -> bool;
        fn video_track_state(track: &NativeVideoTrack) -> u8;
        fn video_track_attach_sink(track: &NativeVideoTrack) -> UniquePtr<NativeVideoSink>;
        fn video_sink_take_frame(sink: &NativeVideoSink) -> UniquePtr<NativeVideoFrame>;
        fn close_video_sink(sink: &NativeVideoSink) -> bool;
        fn peer_add_video_transceiver(
            peer: &NativePeerConnection,
            track: &NativeVideoTrack,
            direction: u8,
            error_type: &mut u8,
            error: &mut String,
        ) -> UniquePtr<NativeRtpTransceiver>;
        fn peer_remove_track(
            peer: &NativePeerConnection,
            sender: &NativeRtpSender,
            error_type: &mut u8,
            error: &mut String,
        ) -> bool;
        fn peer_take_transceiver(
            peer: &NativePeerConnection,
            arrival_id: u64,
        ) -> UniquePtr<NativeRtpTransceiver>;
        fn peer_take_receiver(
            peer: &NativePeerConnection,
            arrival_id: u64,
        ) -> UniquePtr<NativeRtpReceiver>;
        fn rtp_transceiver_sender(transceiver: &NativeRtpTransceiver)
        -> UniquePtr<NativeRtpSender>;
        fn rtp_transceiver_receiver(
            transceiver: &NativeRtpTransceiver,
        ) -> UniquePtr<NativeRtpReceiver>;
        fn rtp_transceiver_direction(transceiver: &NativeRtpTransceiver) -> u8;
        fn rtp_transceiver_current_direction(transceiver: &NativeRtpTransceiver) -> i8;
        fn rtp_transceiver_stopped(transceiver: &NativeRtpTransceiver) -> bool;
        fn rtp_transceiver_set_direction(
            transceiver: &NativeRtpTransceiver,
            direction: u8,
            error_type: &mut u8,
            error: &mut String,
        ) -> bool;
        fn rtp_sender_id(sender: &NativeRtpSender) -> String;
        fn rtp_sender_track(sender: &NativeRtpSender) -> UniquePtr<NativeVideoTrack>;
        fn rtp_receiver_id(receiver: &NativeRtpReceiver) -> String;
        fn rtp_receiver_track(receiver: &NativeRtpReceiver) -> UniquePtr<NativeVideoTrack>;
    }
}

/// Returns the identity shared by this Rust bridge and its precompiled native half.
///
/// The underlying CXX module remains private:
///
/// ```compile_fail
/// use pulsebeam_webrtc_sys::ffi;
/// ```
pub fn bridge_identity() -> &'static str {
    ffi::bridge_identity()
}

#[cfg(test)]
#[path = "../build_support/manifest.rs"]
#[allow(dead_code)]
mod manifest;

#[cfg(test)]
#[path = "../build_support/artifact.rs"]
#[allow(dead_code)]
mod artifact;
