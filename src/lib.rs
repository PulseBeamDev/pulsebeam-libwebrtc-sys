//! Low-level Rust integration with PulseBeam's pinned libwebrtc artifact.

mod audio;
mod audio_processing;
mod codec;
mod data_channel;
mod encoded_video;
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

#[cfg(feature = "native")]
pub use audio::AudioDevice;
pub use audio::{
    AudioCodecCapability, AudioFrameError, AudioPcmFrame, AudioSampleFormat, AudioSink,
    AudioSource, AudioTrack, DecodedAudioFrame, EncodedAudioFrame, EncodedAudioSink,
    EncodedAudioSource, OpusInputError, OpusInputFrame,
};
pub use audio_processing::{
    AudioProcessingConfig, AudioProcessingOptions, AudioProcessingState, GainControl,
    NoiseSuppression, ProcessingChoice, ProcessingComponentState, ProcessingImplementation,
};
pub use codec::{
    AudioDecoderFactory, AudioEncoderFactory, CodecError, CodecParameter, CodecSupport,
    DecodedImageCallback, EncodedImageCallback, EncodedVideoCodec, EncodedVideoFrame,
    EncodedVideoMetadata, Nv12Planes, VideoCodecFormat, VideoDecoder, VideoDecoderFactory,
    VideoDecoderFactoryHandle, VideoDecoderInfo, VideoDecoderSettings, VideoEncoder,
    VideoEncoderFactory, VideoEncoderFactoryHandle, VideoEncoderInfo, VideoEncoderSettings,
    VideoFrame, VideoFrameBuffer, VideoFrameType, VideoPlane, VideoRateControl, VideoResolution,
    VideoRotation,
};
pub use data_channel::{
    DataChannel, DataChannelConfiguration, DataChannelEvent, DataChannelMessage,
    DataChannelMessageKind, DataChannelPriority, DataChannelSendResult, DataChannelState,
};
pub use encoded_video::{
    EncodedH264Input, EncodedH264Source, EncodedVideoAccessUnit, EncodedVideoInput,
    EncodedVideoSource, H264AccessUnit,
};
pub use execution::{
    BuildEnvironmentError, ControlledPeerDriver, ControlledWorld, Environment, EnvironmentBuilder,
    MAX_CONTROLLED_TIME, ManualClock, NetworkThread, PumpResult, QueuePriority, RandomnessLease,
    RandomnessLeaseError, SignalingThread, SystemClock, TaskQueue, TaskQueueFactory,
    ThreadStartError, WorkerThread, WorldAcquireError,
};
pub use network::{
    ControlledSimulatedNetwork, NetworkAddress, NetworkEndpoint, NetworkError,
    NetworkManagerProvider, OutboundKind, OutboundPacket, PacketSocketFactoryProvider,
    ReceivedPacket, SimulatedNetwork, SimulatedUdpSocket,
};
pub use peer::{
    CandidatePairStats, ConnectionState, DataChannelStats, IceCandidate, IceGatheringState,
    IceServer, IceTransportPolicy, InboundRtpStats, OperationCompletion, OperationId,
    OutboundRtpStats, PeerConfiguration, PeerConnection, PeerConnectionEvent,
    PeerConnectionFactory, PeerConnectionFactoryBuilder, PeerDescriptions, PeerError,
    PeerErrorKind, PeerStatsRecord, PeerStatsSnapshot, SessionDescription, SessionDescriptionType,
    SignalingState, TransportStats,
};
#[cfg(feature = "native")]
pub use video::{
    CameraCapture, CameraCaptureStatus, CameraDevice, CameraFormat, CameraPixelFormat,
    ScreenCapture, ScreenCaptureStatus, ScreenSource, WindowCapture, WindowSource,
};
pub use video::{
    DecodeTargetIndication, EncodedReceivedVideoFrame, EncodedVideoSink, RtpReceiver, RtpSender,
    RtpSenderEncoding, RtpSenderParameters, RtpTransceiver, RtpTransceiverDirection,
    VideoCodecCapability, VideoSink, VideoSource, VideoSourceState, VideoTrack, VideoTrackState,
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

    struct FfiVideoCodecCapability {
        format: FfiCodecFormat,
        clock_rate: i32,
        preferred_payload_type: i32,
        rtcp_feedback: Vec<String>,
        scalability_modes: Vec<String>,
    }

    struct FfiAudioCodecCapability {
        format: FfiCodecFormat,
        clock_rate: i32,
        channels: i32,
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
        supports_simulcast: bool,
    }

    struct FfiEncodedVideoMetadata {
        codec: u8,
        simulcast_index: i32,
        spatial_index: i32,
        temporal_index: i32,
        end_of_picture: bool,
        non_reference: bool,
        layer_sync: bool,
        key_index: i32,
        first_frame_in_picture: bool,
        inter_picture_predicted: bool,
        flexible_mode: bool,
        num_spatial_layers: u8,
        inter_layer_predicted: bool,
        temporal_up_switch: bool,
    }

    struct FfiDecoderInfo {
        implementation_name: String,
        hardware_accelerated: bool,
    }

    struct FfiSenderEncoding {
        rid: String,
        active: bool,
        has_max_bitrate: bool,
        max_bitrate_bps: i32,
        has_max_framerate: bool,
        max_framerate: f64,
        has_scale_by: bool,
        scale_by: f64,
        has_scale_to: bool,
        scale_to_width: i32,
        scale_to_height: i32,
        has_scalability_mode: bool,
        scalability_mode: String,
    }

    struct FfiSenderParameters {
        transaction_id: String,
        encodings: Vec<FfiSenderEncoding>,
    }

    struct FfiScreenSource {
        id: i64,
        name: String,
    }

    struct FfiCameraFormat {
        width: u32,
        height: u32,
        max_fps: u32,
        pixel_format: i32,
    }

    struct FfiCameraDevice {
        name: String,
        id: String,
    }

    struct FfiAudioDevice {
        index: u16,
        name: String,
        id: String,
    }

    struct FfiAudioProcessingConfig {
        enabled: bool,
        echo_cancellation: bool,
        noise_suppression: u8,
        gain_control: u8,
    }

    struct FfiAudioProcessingState {
        has_module: bool,
        echo_software: i8,
        echo_platform_available: bool,
        echo_platform: i8,
        echo_effective: u8,
        noise_software: i8,
        noise_platform_available: bool,
        noise_platform: i8,
        noise_effective: u8,
        gain_software: i8,
        gain_platform_available: bool,
        gain_platform: i8,
        gain_effective: u8,
        highpass_software: i8,
        highpass_platform_available: bool,
        highpass_platform: i8,
        highpass_effective: u8,
    }

    struct FfiIceServer {
        url: String,
        username: String,
        password: String,
    }

    struct FfiDescriptionSnapshot {
        slot: u8,
        kind: u8,
        sdp: String,
    }

    struct FfiStatsField {
        name: String,
        kind: u8,
        text: String,
        unsigned_value: u64,
        signed_value: i64,
        decimal: f64,
        flag: bool,
    }

    struct FfiStatsRecord {
        id: String,
        kind: u8,
        timestamp_us: i64,
        fields: Vec<FfiStatsField>,
    }

    struct FfiPeerEvent {
        stats_timestamp_us: i64,
        stats_records: Vec<FfiStatsRecord>,
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
            presentation_token: i64,
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

    struct FfiEncodedVideoFrame {
        available: bool,
        data: Vec<u8>,
        mime_type: String,
        rtp_timestamp: u32,
        ssrc: u32,
        payload_type: u8,
        key_frame: bool,
        rid: String,
        has_rid: bool,
        has_capture_time: bool,
        capture_time_us: i64,
        has_receive_time: bool,
        receive_time_us: i64,
        has_frame_id: bool,
        frame_id: i64,
        spatial_index: i32,
        temporal_index: i32,
        dependencies: Vec<i64>,
        decode_target_indications: Vec<u8>,
    }

    struct FfiReceivedAudioFrame {
        valid: bool,
        sample_rate_hz: u32,
        channels: u8,
        samples_per_channel: u32,
        has_capture_time: bool,
        capture_time_us: i64,
        samples: Vec<i16>,
    }

    struct FfiEncodedAudioFrame {
        available: bool,
        data: Vec<u8>,
        rtp_timestamp: u32,
        ssrc: u32,
        payload_type: u8,
        samples_per_channel: u32,
        has_capture_time: bool,
        capture_time_us: i64,
        has_receive_time: bool,
        receive_time_us: i64,
    }

    unsafe extern "C++" {
        include!("pulsebeam-webrtc-sys/native/probe.h");
        include!("pulsebeam-webrtc-sys/native/execution.h");
        include!("pulsebeam-webrtc-sys/native/network.h");
        include!("pulsebeam-webrtc-sys/native/codec.h");
        include!("pulsebeam-webrtc-sys/native/peer.h");
        include!("pulsebeam-webrtc-sys/native/data_channel.h");
        include!("pulsebeam-webrtc-sys/native/video.h");
        include!("pulsebeam-webrtc-sys/native/audio.h");

        type NativeEnvironment;
        type NativeManualClock;
        type NativeRandomnessLease;
        type NativeTaskQueue;
        type NativeTaskQueueFactory;
        type NativeDriverThread;
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
        type NativeScreenCapture;
        type NativeCamera;
        type NativeVideoSource;
        type NativeVideoTrack;
        type NativeAudioSource;
        type NativeAudioTrack;
        type NativeAudioSink;
        type NativeEncodedAudioSink;
        type NativeVideoSink;
        type NativeEncodedVideoSink;
        type NativeRtpSender;
        type NativeRtpReceiver;
        type NativeRtpTransceiver;
        type NativeTransceiverList;

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
        fn task_queue_factory_is_current(factory: &NativeTaskQueueFactory) -> bool;
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
        fn pump_ready_tasks(factory: &NativeTaskQueueFactory, budget: usize) -> usize;
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
        fn new_driver_thread(clock: &NativeManualClock) -> UniquePtr<NativeDriverThread>;
        fn new_seeded_driver_thread(
            clock: &NativeManualClock,
            seed: u64,
        ) -> UniquePtr<NativeDriverThread>;
        fn driver_pump(driver: &NativeDriverThread, budget: usize) -> usize;
        fn borrow_driver_thread(driver: &NativeDriverThread) -> UniquePtr<NativeThread>;
        fn driver_run_ready(driver: &NativeDriverThread) -> bool;
        fn test_driver_lifecycle_yield(
            driver: &NativeDriverThread,
            factory: &NativeTaskQueueFactory,
        ) -> bool;
        fn driver_next_deadline_us(driver: &NativeDriverThread) -> i64;
        fn driver_is_current(driver: &NativeDriverThread) -> bool;
        fn thread_post_task(thread: &NativeThread, task: Box<RustTask>) -> bool;
        fn thread_post_delayed_task(
            thread: &NativeThread,
            delay_us: i64,
            task: Box<RustTask>,
        ) -> bool;

        fn new_simulated_network(clock: &NativeManualClock) -> UniquePtr<NativeSimulatedNetwork>;
        fn new_controlled_simulated_network(
            clock: &NativeManualClock,
            driver: &NativeDriverThread,
        ) -> UniquePtr<NativeSimulatedNetwork>;
        fn add_simulated_dns_record(
            network: &NativeSimulatedNetwork,
            hostname: &str,
            ip: &[u8],
        ) -> bool;
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
        fn outbound_packet_kind(packet: &NativeOutboundPacket) -> u8;
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
        fn inject_simulated_tcp_data(
            network: &NativeSimulatedNetwork,
            source_ip: &[u8],
            source_port: u16,
            destination_ip: &[u8],
            destination_port: u16,
            data: &[u8],
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
        fn new_opus_carrier_audio_encoder_factory() -> UniquePtr<NativeAudioEncoderFactory>;
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
        fn native_video_frame_rotation(frame: &NativeVideoFrame) -> u16;
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
            metadata: &FfiEncodedVideoMetadata,
        ) -> bool;
        fn decoded_callback_emit(
            callback: &NativeDecodedImageCallback,
            data: &[u8],
            width: u32,
            height: u32,
            timestamp_us: i64,
            rtp_timestamp: u32,
            rotation: u16,
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
            native_audio: bool,
            processing: &FfiAudioProcessingConfig,
            error: &mut String,
        ) -> UniquePtr<NativePeerConnectionFactory>;
        fn factory_audio_processing_state(
            factory: &NativePeerConnectionFactory,
        ) -> FfiAudioProcessingState;
        fn factory_audio_devices(
            factory: &NativePeerConnectionFactory,
            recording: bool,
            devices: &mut Vec<FfiAudioDevice>,
            error: &mut String,
        ) -> bool;
        fn factory_select_audio_device(
            factory: &NativePeerConnectionFactory,
            recording: bool,
            index: u16,
            error: &mut String,
        ) -> bool;
        fn create_peer_connection(
            factory: &NativePeerConnectionFactory,
            ice_candidate_pool_size: u16,
            always_negotiate_data_channels: bool,
            ice_servers: &[FfiIceServer],
            relay_only: bool,
            turn_tls_ca_pem: &str,
            error: &mut String,
        ) -> UniquePtr<NativePeerConnection>;
        fn peer_create_offer(peer: &NativePeerConnection, operation_id: u64, ice_restart: bool);
        fn peer_create_answer(peer: &NativePeerConnection, operation_id: u64);
        fn peer_request_stats(peer: &NativePeerConnection, operation_id: u64) -> bool;
        fn peer_descriptions(
            peer: &NativePeerConnection,
            descriptions: &mut Vec<FfiDescriptionSnapshot>,
        ) -> u8;
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
        fn peer_reject_controlled_media(peer: &NativePeerConnection, operation_id: u64);
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
        fn peer_set_native_audio_enabled(
            peer: &NativePeerConnection,
            recording: bool,
            enabled: bool,
            error: &mut String,
        ) -> bool;

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

        fn audio_source_push_opus(
            source: &NativeAudioSource,
            payload: &[u8],
            rtp_timestamp: u32,
            samples_per_channel: u32,
        ) -> bool;
        fn create_audio_source(
            factory: &NativePeerConnectionFactory,
        ) -> UniquePtr<NativeAudioSource>;
        fn create_encoded_audio_source(
            factory: &NativePeerConnectionFactory,
            channels: u8,
        ) -> UniquePtr<NativeAudioSource>;
        fn peer_audio_codec_capabilities(
            factory: &NativePeerConnectionFactory,
            sender: bool,
        ) -> Vec<FfiAudioCodecCapability>;
        fn rtp_transceiver_set_audio_codec_preferences(
            transceiver: &NativeRtpTransceiver,
            factory: &NativePeerConnectionFactory,
            formats: &[FfiCodecFormat],
            error_type: &mut u8,
            error: &mut String,
        ) -> bool;
        fn close_audio_source(source: &NativeAudioSource) -> bool;
        fn audio_source_push_pcm(
            source: &NativeAudioSource,
            samples: &[i16],
            sample_rate_hz: u32,
            channels: u8,
            timestamp_us: i64,
        ) -> bool;
        fn create_microphone_track(
            factory: &NativePeerConnectionFactory,
            id: &str,
        ) -> UniquePtr<NativeAudioTrack>;
        fn create_audio_track(
            factory: &NativePeerConnectionFactory,
            source: &NativeAudioSource,
            id: &str,
        ) -> UniquePtr<NativeAudioTrack>;
        fn audio_track_id(track: &NativeAudioTrack) -> String;
        fn audio_track_enabled(track: &NativeAudioTrack) -> bool;
        fn audio_track_set_enabled(track: &NativeAudioTrack, enabled: bool) -> bool;
        fn audio_track_set_processing_options(
            track: &NativeAudioTrack,
            echo: u8,
            noise: u8,
            gain: u8,
            error: &mut String,
        ) -> bool;
        fn peer_add_audio_transceiver(
            peer: &NativePeerConnection,
            track: &NativeAudioTrack,
            direction: u8,
            error_type: &mut u8,
            error: &mut String,
        ) -> UniquePtr<NativeRtpTransceiver>;
        fn rtp_receiver_attach_audio_sink(
            peer: &NativePeerConnection,
            receiver: &NativeRtpReceiver,
        ) -> UniquePtr<NativeAudioSink>;
        fn audio_sink_take_frame(sink: &NativeAudioSink) -> FfiReceivedAudioFrame;
        fn audio_sink_dropped_frames(sink: &NativeAudioSink) -> u64;
        fn close_audio_sink(sink: &NativeAudioSink) -> bool;
        fn rtp_receiver_attach_encoded_audio_sink(
            peer: &NativePeerConnection,
            receiver: &NativeRtpReceiver,
        ) -> UniquePtr<NativeEncodedAudioSink>;
        fn encoded_audio_sink_take_frame(sink: &NativeEncodedAudioSink) -> FfiEncodedAudioFrame;
        fn encoded_audio_sink_dropped_frames(sink: &NativeEncodedAudioSink) -> u64;
        fn close_encoded_audio_sink(sink: &NativeEncodedAudioSink) -> bool;

        fn screen_sources(
            windows: bool,
            screens: &mut Vec<FfiScreenSource>,
            error: &mut String,
        ) -> bool;
        fn open_screen(
            source: &NativeVideoSource,
            screen_id: i64,
            window: bool,
            error: &mut String,
        ) -> UniquePtr<NativeScreenCapture>;
        fn screen_capture_next_frame(capture: &NativeScreenCapture) -> bool;
        fn screen_capture_status(capture: &NativeScreenCapture) -> u8;
        fn screen_capture_failed_frames(capture: &NativeScreenCapture) -> u64;
        fn close_screen(capture: &NativeScreenCapture) -> bool;
        fn camera_devices(devices: &mut Vec<FfiCameraDevice>, error: &mut String) -> bool;
        fn camera_formats(
            device_id: &str,
            formats: &mut Vec<FfiCameraFormat>,
            error: &mut String,
        ) -> bool;
        fn open_camera(
            source: &NativeVideoSource,
            device_id: &str,
            width: u32,
            height: u32,
            fps: u32,
            error: &mut String,
        ) -> UniquePtr<NativeCamera>;
        fn camera_capture_status(camera: &NativeCamera, stale_after_ms: u64) -> u8;
        fn close_camera(camera: &NativeCamera) -> bool;
        fn create_video_source(
            factory: &NativePeerConnectionFactory,
        ) -> UniquePtr<NativeVideoSource>;
        fn close_video_source(source: &NativeVideoSource) -> bool;
        fn video_source_state(source: &NativeVideoSource) -> u8;
        fn video_source_push_encoded_trigger(
            source: &NativeVideoSource,
            width: u32,
            height: u32,
            timestamp_us: i64,
            token: i64,
        ) -> bool;
        fn video_source_push_frame(
            source: &NativeVideoSource,
            data: &[u8],
            width: u32,
            height: u32,
            timestamp_us: i64,
            rtp_timestamp: u32,
            rotation: u16,
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
        fn video_sink_dropped_frames(sink: &NativeVideoSink) -> u64;
        fn close_video_sink(sink: &NativeVideoSink) -> bool;
        fn peer_audio_transceivers(peer: &NativePeerConnection)
        -> UniquePtr<NativeTransceiverList>;
        fn peer_video_transceivers(peer: &NativePeerConnection)
        -> UniquePtr<NativeTransceiverList>;
        fn peer_video_codec_capabilities(
            factory: &NativePeerConnectionFactory,
            sender: bool,
        ) -> Vec<FfiVideoCodecCapability>;
        fn transceiver_list_len(list: &NativeTransceiverList) -> usize;
        fn transceiver_list_at(
            list: &NativeTransceiverList,
            index: usize,
        ) -> UniquePtr<NativeRtpTransceiver>;
        fn peer_add_video_transceiver(
            peer: &NativePeerConnection,
            track: &NativeVideoTrack,
            direction: u8,
            rids: &[String],
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
        fn rtp_transceiver_mid(transceiver: &NativeRtpTransceiver, mid: &mut String) -> bool;
        fn rtp_transceiver_set_video_codec_preferences(
            transceiver: &NativeRtpTransceiver,
            factory: &NativePeerConnectionFactory,
            formats: &[FfiCodecFormat],
            error_type: &mut u8,
            error: &mut String,
        ) -> bool;
        fn rtp_transceiver_stop(
            transceiver: &NativeRtpTransceiver,
            error_type: &mut u8,
            error: &mut String,
        ) -> bool;
        fn rtp_transceiver_set_direction(
            transceiver: &NativeRtpTransceiver,
            direction: u8,
            error_type: &mut u8,
            error: &mut String,
        ) -> bool;
        fn rtp_sender_id(sender: &NativeRtpSender) -> String;
        fn rtp_sender_request_keyframe(
            sender: &NativeRtpSender,
            peer: &NativePeerConnection,
            rids: &[String],
            error_type: &mut u8,
            error: &mut String,
        ) -> bool;
        fn rtp_sender_get_parameters(
            sender: &NativeRtpSender,
            parameters: &mut FfiSenderParameters,
        ) -> bool;
        fn rtp_sender_set_parameters(
            sender: &NativeRtpSender,
            parameters: &FfiSenderParameters,
            error_type: &mut u8,
            error: &mut String,
        ) -> bool;
        fn rtp_sender_track(sender: &NativeRtpSender) -> UniquePtr<NativeVideoTrack>;
        fn rtp_sender_set_video_track(sender: &NativeRtpSender, track: &NativeVideoTrack) -> bool;
        fn rtp_sender_clear_track(sender: &NativeRtpSender) -> bool;
        fn rtp_receiver_id(receiver: &NativeRtpReceiver) -> String;
        fn rtp_receiver_track(receiver: &NativeRtpReceiver) -> UniquePtr<NativeVideoTrack>;
        fn rtp_receiver_request_keyframe(
            peer: &NativePeerConnection,
            receiver: &NativeRtpReceiver,
        ) -> bool;
        fn rtp_receiver_attach_encoded_video_sink(
            peer: &NativePeerConnection,
            receiver: &NativeRtpReceiver,
        ) -> UniquePtr<NativeEncodedVideoSink>;
        fn encoded_video_sink_take_frame(sink: &NativeEncodedVideoSink) -> FfiEncodedVideoFrame;
        fn encoded_video_sink_dropped_frames(sink: &NativeEncodedVideoSink) -> u64;
        fn close_encoded_video_sink(sink: &NativeEncodedVideoSink) -> bool;
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
