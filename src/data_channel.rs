use std::{fmt, rc::Rc};

use crate::{PeerConnection, PeerError, PeerErrorKind, ffi, peer::PeerInner};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i8)]
pub enum DataChannelPriority {
    VeryLow = 0,
    Low = 1,
    Medium = 2,
    High = 3,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataChannelConfiguration {
    pub ordered: bool,
    pub max_retransmit_time_ms: Option<u16>,
    pub max_retransmits: Option<u16>,
    pub protocol: String,
    pub negotiated: bool,
    pub id: Option<u16>,
    pub priority: Option<DataChannelPriority>,
}

impl Default for DataChannelConfiguration {
    fn default() -> Self {
        Self {
            ordered: true,
            max_retransmit_time_ms: None,
            max_retransmits: None,
            protocol: String::new(),
            negotiated: false,
            id: None,
            priority: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataChannelState {
    Connecting,
    Open,
    Closing,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataChannelMessageKind {
    Text,
    Binary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataChannelMessage {
    pub kind: DataChannelMessageKind,
    pub bytes: Vec<u8>,
}

impl DataChannelMessage {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            kind: DataChannelMessageKind::Text,
            bytes: text.into().into_bytes(),
        }
    }

    pub fn binary(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            kind: DataChannelMessageKind::Binary,
            bytes: bytes.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataChannelSendResult {
    Sent,
    NotOpen,
    Backpressure,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataChannelEvent {
    StateChanged(DataChannelState),
    Message(DataChannelMessage),
    BufferedAmountChanged { sent_data_size: u64 },
}

/// A sequence-bound data channel with a caller-polled owned event queue.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<pulsebeam_webrtc_sys::DataChannel>();
/// ```
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<pulsebeam_webrtc_sys::DataChannel>();
/// ```
pub struct DataChannel {
    native: cxx::UniquePtr<ffi::NativeDataChannel>,
    _peer: Rc<PeerInner>,
}

impl fmt::Debug for DataChannel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DataChannel")
            .field("label", &self.label())
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}

impl PeerConnection {
    pub fn create_data_channel(
        &self,
        label: &str,
        configuration: DataChannelConfiguration,
    ) -> Result<DataChannel, PeerError> {
        if configuration.max_retransmit_time_ms.is_some() && configuration.max_retransmits.is_some()
        {
            return Err(PeerError {
                kind: PeerErrorKind::InvalidParameter,
                message: "max retransmit time and max retransmits are mutually exclusive".into(),
            });
        }
        if configuration.negotiated != configuration.id.is_some() {
            return Err(PeerError {
                kind: PeerErrorKind::InvalidParameter,
                message: "negotiated channels require an id and in-band channels must omit it"
                    .into(),
            });
        }
        let mut error_type = 0;
        let mut message = String::new();
        let native = ffi::create_data_channel(
            self.inner.native(),
            label,
            configuration.ordered,
            configuration.max_retransmit_time_ms.map_or(-1, i32::from),
            configuration.max_retransmits.map_or(-1, i32::from),
            &configuration.protocol,
            configuration.negotiated,
            configuration.id.map_or(-1, i32::from),
            configuration.priority.map_or(-1, |value| value as i8),
            &mut error_type,
            &mut message,
        );
        if native.is_null() {
            Err(PeerError {
                kind: super::peer::error_kind(error_type),
                message,
            })
        } else {
            Ok(DataChannel::from_native(native, self.inner.clone()))
        }
    }
}

impl DataChannel {
    pub(crate) fn from_native(
        native: cxx::UniquePtr<ffi::NativeDataChannel>,
        peer: Rc<PeerInner>,
    ) -> Self {
        Self {
            native,
            _peer: peer,
        }
    }

    pub fn label(&self) -> String {
        ffi::data_channel_label(self.native())
    }

    pub fn configuration(&self) -> DataChannelConfiguration {
        DataChannelConfiguration {
            ordered: ffi::data_channel_ordered(self.native()),
            max_retransmit_time_ms: optional_u16(ffi::data_channel_max_retransmit_time_ms(
                self.native(),
            )),
            max_retransmits: optional_u16(ffi::data_channel_max_retransmits(self.native())),
            protocol: ffi::data_channel_protocol(self.native()),
            negotiated: ffi::data_channel_negotiated(self.native()),
            id: optional_u16(ffi::data_channel_id(self.native())),
            priority: Some(priority(ffi::data_channel_priority(self.native()))),
        }
    }

    pub fn state(&self) -> DataChannelState {
        state(ffi::data_channel_state(self.native()))
    }

    pub fn buffered_amount(&self) -> u64 {
        ffi::data_channel_buffered_amount(self.native())
    }

    pub fn send(&self, message: DataChannelMessage) -> DataChannelSendResult {
        match ffi::data_channel_send(
            self.native(),
            &message.bytes,
            message.kind == DataChannelMessageKind::Binary,
        ) {
            0 => DataChannelSendResult::Sent,
            1 => DataChannelSendResult::NotOpen,
            2 => DataChannelSendResult::Backpressure,
            3 => DataChannelSendResult::Failed,
            _ => unreachable!("native adapter returned an invalid send result"),
        }
    }

    pub fn try_next_event(&self) -> Option<DataChannelEvent> {
        let event = ffi::data_channel_take_event(self.native());
        match event.kind {
            0 => None,
            1 => Some(DataChannelEvent::StateChanged(state(event.state))),
            2 => Some(DataChannelEvent::Message(DataChannelMessage {
                kind: if event.binary {
                    DataChannelMessageKind::Binary
                } else {
                    DataChannelMessageKind::Text
                },
                bytes: event.data,
            })),
            3 => Some(DataChannelEvent::BufferedAmountChanged {
                sent_data_size: event.sent_data_size,
            }),
            _ => unreachable!("native adapter returned an invalid data-channel event"),
        }
    }

    pub fn close(&mut self) -> Result<(), PeerError> {
        ffi::close_data_channel(self.native())
            .then_some(())
            .ok_or_else(|| PeerError {
                kind: PeerErrorKind::Internal,
                message: "failed to close data channel".into(),
            })
    }

    fn native(&self) -> &ffi::NativeDataChannel {
        self.native.as_ref().expect("validated data channel")
    }
}

fn optional_u16(value: i32) -> Option<u16> {
    (value >= 0).then(|| value as u16)
}

fn state(value: u8) -> DataChannelState {
    match value {
        0 => DataChannelState::Connecting,
        1 => DataChannelState::Open,
        2 => DataChannelState::Closing,
        3 => DataChannelState::Closed,
        _ => unreachable!("native adapter returned an invalid data-channel state"),
    }
}

fn priority(value: u8) -> DataChannelPriority {
    match value {
        0 => DataChannelPriority::VeryLow,
        1 => DataChannelPriority::Low,
        2 => DataChannelPriority::Medium,
        3 => DataChannelPriority::High,
        _ => unreachable!("native adapter returned an invalid data-channel priority"),
    }
}
