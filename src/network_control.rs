//! Owned native congestion-controller injection, including callable products.
use crate::network_control_values::*;
use crate::{EventLogEnvironment, FieldTrialsSnapshot, TimeDelta, Timestamp, ffi};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
};

/// Retained creation environment, independent of factory or actor lifetime.
pub struct NetworkControllerEnvironment(EventLogEnvironment);
impl NetworkControllerEnvironment {
    pub fn time_us(&self) -> i64 {
        self.0.time_us()
    }
    pub fn timestamp(&self) -> Timestamp {
        self.0.timestamp()
    }
    pub fn field_trials(&self) -> FieldTrialsSnapshot {
        self.0.field_trials()
    }
    pub fn environment(&self) -> &crate::Environment {
        self.0.environment()
    }
}
pub struct NetworkControllerConfig {
    pub environment: NetworkControllerEnvironment,
    pub constraints: TargetRateConstraints,
    pub stream_based_config: StreamsConfig,
    pub default_pacing_time_window: TimeDelta,
}
/// Full non-concurrent SDK product contract. The bridge serializes calls,
/// so providers need Send, not Sync. Never block or reenter the calling session.
/// Controlled providers must do only caller-driven synchronous work.
/// Return native-valid updates; invalid unit construction latches product
/// failure before consumption. Failed products command a zero transfer-rate target;
/// engine-owned queued packets retain the last valid pacing configuration.
pub trait NetworkController: Send + 'static {
    fn on_network_availability(&mut self, value: NetworkAvailability) -> NetworkControlUpdate;
    fn on_network_route_change(&mut self, value: NetworkRouteChange) -> NetworkControlUpdate;
    fn on_process_interval(&mut self, value: ProcessInterval) -> NetworkControlUpdate;
    fn on_remote_bitrate_report(&mut self, value: RemoteBitrateReport) -> NetworkControlUpdate;
    fn on_round_trip_time_update(&mut self, value: RoundTripTimeUpdate) -> NetworkControlUpdate;
    fn on_sent_packet(&mut self, value: SentPacket) -> NetworkControlUpdate;
    fn on_received_packet(&mut self, value: ReceivedPacket) -> NetworkControlUpdate;
    fn on_streams_config(&mut self, value: StreamsConfig) -> NetworkControlUpdate;
    fn on_target_rate_constraints(&mut self, value: TargetRateConstraints) -> NetworkControlUpdate;
    fn on_transport_loss_report(&mut self, value: TransportLossReport) -> NetworkControlUpdate;
    fn on_transport_packets_feedback(
        &mut self,
        value: TransportPacketsFeedback,
    ) -> NetworkControlUpdate;
    fn on_network_state_estimate(&mut self, value: NetworkStateEstimate) -> NetworkControlUpdate;
    fn supports_ecn_adaptation(&mut self) -> bool {
        false
    }
}
/// The SDK send controller requires a non-null product. Factory creation and
/// interval queries are serialized, including const native interval queries.
/// Non-finite intervals disable periodic updates; finite intervals must not be
/// negative. Do not wait for actor work inside native callbacks.
pub trait NetworkControllerFactoryProvider: Send + 'static {
    fn create(&mut self, config: NetworkControllerConfig) -> Box<dyn NetworkController>;
    fn process_interval(&mut self) -> TimeDelta;
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkControllerFailure {
    Panicked,
    InvalidUnits,
    InvalidProcessInterval,
    InvalidUpdate,
}
fn decode_failure(value: u8) -> Option<NetworkControllerFailure> {
    match value {
        1 => Some(NetworkControllerFailure::Panicked),
        2 => Some(NetworkControllerFailure::InvalidUnits),
        3 => Some(NetworkControllerFailure::InvalidProcessInterval),
        4 => Some(NetworkControllerFailure::InvalidUpdate),
        _ => None,
    }
}
struct FactoryState {
    provider: Mutex<Option<Box<dyn NetworkControllerFactoryProvider>>>,
    failed: Arc<AtomicBool>,
    observed_failure: Arc<AtomicU8>,
}
impl Drop for FactoryState {
    fn drop(&mut self) {
        let provider = self
            .provider
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        let _ = catch_unwind(AssertUnwindSafe(|| drop(provider)));
    }
}
/// Owned registration. Pinned peer factories use this only with the
/// WebRTC-Bwe-InjectedCongestionController field trial enabled; builders reject
/// disabled injection instead of silently accepting an unused provider.
#[derive(Clone)]
pub struct NetworkControllerFactory(Arc<FactoryState>);
impl NetworkControllerFactory {
    pub fn from_provider(provider: impl NetworkControllerFactoryProvider) -> Self {
        Self(Arc::new(FactoryState {
            provider: Mutex::new(Some(Box::new(provider))),
            failed: Arc::new(AtomicBool::new(false)),
            observed_failure: Arc::new(AtomicU8::new(0)),
        }))
    }
    /// Factory-level failure; an individual product failure is isolated.
    pub fn has_failed(&self) -> bool {
        self.0.failed.load(Ordering::Acquire)
    }
    /// First observed bridge failure, including product validation/panics.
    pub fn observed_failure(&self) -> Option<NetworkControllerFailure> {
        decode_failure(self.0.observed_failure.load(Ordering::Acquire))
    }
}
fn report(observer: &AtomicU8, value: u8) {
    let _ = observer.compare_exchange(0, value, Ordering::AcqRel, Ordering::Acquire);
}
pub(crate) struct RustNetworkControllerFactory(pub Option<NetworkControllerFactory>, pub Option<crate::Environment>);
pub(crate) struct RustNetworkController {
    provider: Mutex<Option<Box<dyn NetworkController>>>,
    failed: AtomicBool,
    factory_failed: Arc<AtomicBool>,
    observed_failure: Arc<AtomicU8>,
    _environment: EventLogEnvironment,
}
impl Drop for RustNetworkController {
    fn drop(&mut self) {
        let provider = self
            .provider
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        let _ = catch_unwind(AssertUnwindSafe(|| drop(provider)));
    }
}
pub(crate) fn network_controller_factory_present(source: &RustNetworkControllerFactory) -> bool {
    source.0.is_some()
}
pub(crate) fn network_controller_create(
    source: &RustNetworkControllerFactory,
    environment: cxx::UniquePtr<ffi::NativeEnvironment>,
    config: ffi::FfiNcConfig,
) -> Box<RustNetworkController> {
    let environment = EventLogEnvironment::from_native(environment, source.1.as_ref());
    let mut product = None;
    let observer = source
        .0
        .as_ref()
        .map(|factory| factory.0.observed_failure.clone())
        .unwrap_or_else(|| Arc::new(AtomicU8::new(0)));
    if let Some(factory) = &source.0 {
        let mut provider = factory
            .0
            .provider
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if !factory.has_failed() {
            let config = NetworkControllerConfig {
                environment: NetworkControllerEnvironment(environment.clone()),
                constraints: TargetRateConstraints::from_native(config.constraints),
                stream_based_config: StreamsConfig::from_native(config.stream_based_config),
                default_pacing_time_window: TimeDelta::from_native_microsecond_bits(
                    config.default_pacing_time_window,
                ),
            };
            match catch_unwind(AssertUnwindSafe(|| {
                provider
                    .as_deref_mut()
                    .map(|provider| provider.create(config))
            })) {
                Ok(value) => product = value,
                Err(_) => {
                    factory.0.failed.store(true, Ordering::Release);
                    report(&observer, 1);
                }
            }
        }
    }
    let failed = product.is_none();
    let factory_failed = source
        .0
        .as_ref()
        .map(|factory| factory.0.failed.clone())
        .unwrap_or_else(|| Arc::new(AtomicBool::new(true)));
    Box::new(RustNetworkController {
        provider: Mutex::new(product),
        failed: AtomicBool::new(failed),
        _environment: environment,
        factory_failed,
        observed_failure: observer,
    })
}
pub(crate) fn network_controller_interval(source: &RustNetworkControllerFactory) -> i64 {
    let Some(factory) = &source.0 else {
        return i64::MAX;
    };
    let mut provider = factory
        .0
        .provider
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if factory.has_failed() {
        return i64::MAX;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        provider
            .as_deref_mut()
            .map(|provider| provider.process_interval())
    }));
    match result {
        Ok(Some(value)) if !value.is_finite() || value.microseconds_or(0) >= 0 => {
            value.native_microsecond_bits()
        }
        result => {
            factory.0.failed.store(true, Ordering::Release);
            report(
                &factory.0.observed_failure,
                if result.is_err() { 1 } else { 3 },
            );
            i64::MAX
        }
    }
}
impl RustNetworkController {
    fn with<T>(&self, fallback: T, call: impl FnOnce(&mut dyn NetworkController) -> T) -> T {
        if self.failed.load(Ordering::Acquire) || self.factory_failed.load(Ordering::Acquire) {
            return fallback;
        }
        let mut provider = self
            .provider
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.failed.load(Ordering::Acquire) || self.factory_failed.load(Ordering::Acquire) {
            return fallback;
        }
        let Some(provider) = provider.as_deref_mut() else {
            return fallback;
        };
        match catch_unwind(AssertUnwindSafe(|| call(provider))) {
            Ok(value) => value,
            Err(_) => {
                self.failed.store(true, Ordering::Release);
                report(&self.observed_failure, 1);
                fallback
            }
        }
    }
}
pub(crate) fn network_controller_invalid(source: &RustNetworkController, kind: u8) {
    source.failed.store(true, Ordering::Release);
    report(&source.observed_failure, kind);
}
pub(crate) fn network_controller_ecn(source: &RustNetworkController) -> bool {
    source.with(false, |provider| provider.supports_ecn_adaptation())
}
#[cfg(test)]
#[path = "network_control_tests.rs"]
mod tests;

pub(crate) fn network_controller_update(
    source: &RustNetworkController,
    kind: u8,
    message: ffi::FfiNcMessage,
) -> ffi::FfiNcNetworkControlUpdate {
    source.with(
        ffi::FfiNcNetworkControlUpdate {
            failed: true,
            ..Default::default()
        },
        |provider| {
            let update = match kind {
                0 => provider.on_network_availability(NetworkAvailability::from_native(
                    message.network_availability,
                )),
                1 => provider.on_network_route_change(NetworkRouteChange::from_native(
                    message.network_route_change,
                )),
                2 => provider
                    .on_process_interval(ProcessInterval::from_native(message.process_interval)),
                3 => provider.on_remote_bitrate_report(RemoteBitrateReport::from_native(
                    message.remote_bitrate_report,
                )),
                4 => provider.on_round_trip_time_update(RoundTripTimeUpdate::from_native(
                    message.round_trip_time_update,
                )),
                5 => provider.on_sent_packet(SentPacket::from_native(message.sent_packet)),
                6 => provider
                    .on_received_packet(ReceivedPacket::from_native(message.received_packet)),
                7 => provider.on_streams_config(StreamsConfig::from_native(message.streams_config)),
                8 => provider.on_target_rate_constraints(TargetRateConstraints::from_native(
                    message.target_rate_constraints,
                )),
                9 => provider.on_transport_loss_report(TransportLossReport::from_native(
                    message.transport_loss_report,
                )),
                10 => provider.on_transport_packets_feedback(
                    TransportPacketsFeedback::from_native(message.transport_packets_feedback),
                ),
                11 => provider.on_network_state_estimate(NetworkStateEstimate::from_native(
                    message.network_state_estimate,
                )),
                _ => {
                    return ffi::FfiNcNetworkControlUpdate {
                        failed: true,
                        ..Default::default()
                    };
                }
            };
            update.native()
        },
    )
}
