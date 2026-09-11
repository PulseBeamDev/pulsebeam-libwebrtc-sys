//! Low-level Rust integration with PulseBeam's pinned libwebrtc artifact.

mod execution;
mod network;

pub(crate) use execution::{RustTask, run_task};

pub use execution::{
    BuildEnvironmentError, Environment, EnvironmentBuilder, ManualClock, NetworkThread,
    QueuePriority, RandomnessLease, RandomnessLeaseError, SignalingThread, SystemClock, TaskQueue,
    TaskQueueFactory, ThreadStartError, WorkerThread,
};
pub use network::{
    NetworkAddress, NetworkEndpoint, NetworkError, NetworkManagerProvider, OutboundPacket,
    PacketSocketFactoryProvider, ReceivedPacket, SimulatedNetwork, SimulatedUdpSocket,
};

#[cxx::bridge(namespace = "pulsebeam::webrtc_sys")]
mod ffi {
    extern "Rust" {
        type RustTask;

        fn run_task(task: Box<RustTask>);
    }

    unsafe extern "C++" {
        include!("pulsebeam-webrtc-sys/native/probe.h");
        include!("pulsebeam-webrtc-sys/native/execution.h");
        include!("pulsebeam-webrtc-sys/native/network.h");

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
