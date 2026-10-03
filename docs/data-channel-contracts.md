# Data-channel public contract mapping

Native authority: WebRTC revision `ba469aa2093ba950066258ca0a59a6fbd1295582`,
`api/data_channel_interface.h`, with `api/priority.h`, `api/rtc_error.h`, and
`api/sctp_transport_interface.h` for dependent values. This inventory describes
only the data-channel family, not the complete peer, transport, or media APIs.
The bridge in `src/lib.rs` is private. Consumers use `src/data_channel.rs` and
the owned operations in `src/production.rs`.

## Construction and values

| Native contract | Rust representation and behavior | Public consumer coverage |
| --- | --- | --- |
| `DataChannelInit::ordered` | `DataChannelConfiguration::ordered`, native default `true` | `tests/data_channel.rs`: `remote_and_negotiated_channels_exchange_owned_bytes_deterministically` |
| `maxRetransmitTime`, `maxRetransmits` | Optional `u16` millisecond lifetime/count. Native effective values are bounded to this range; negative native inputs mean absence. Setting both is rejected explicitly. | `invalid_data_channel_configuration_is_explicit`, `remote_and_negotiated_channels_exchange_owned_bytes_deterministically` |
| Deprecated `reliable` init field | No independent setting: the engine derives reliability from retransmission options. `DataChannel::reliable()` queries the native property. | `async_native_completion_size_and_counters` |
| `protocol`, channel label | Owned strings; protocol remains opaque to the binding. Native errors validate engine constraints. | `remote_and_negotiated_channels_exchange_owned_bytes_deterministically` |
| `negotiated`, `id` | Boolean and `Option<u16>`. Native assignment is preserved for an absent id, including negotiated channels. In-band explicit ids are ignored natively. Reserved SID 65535 is rejected by the engine. | `invalid_data_channel_configuration_is_explicit`, `native_stream_ids_can_be_reused_after_reset` |
| Optional `PriorityValue` | `Option<DataChannelPriority>`, four named presets and arbitrary `Custom(u16)`. Reading configuration reports effective priority, not the originally unset option. | `invalid_data_channel_configuration_is_explicit` |
| `DataBuffer` bytes, binary flag, size | `DataChannelMessage` owns bytes and a text/binary tag; byte vector length is size. Received text is not assumed valid UTF-8. | `remote_and_negotiated_channels_exchange_owned_bytes_deterministically` |
| Four `DataState` values and `DataStateString` helper | `DataChannelState::{Connecting, Open, Closing, Closed}`; idiomatic enum debugging replaces the native string helper. | `close_and_drop_orders_quiesce_observers_and_reject_sends` |
| `RTCError` type, detail, message, SCTP cause | Owned `DataChannelError`; `None` denotes no native error. Typed `PeerErrorKind`/`DataChannelErrorDetail`, unknown detail values retained, and optional `u16` SCTP cause. | `native_pre_observer_overflow_is_an_owned_resource_error`, `async_native_completion_size_and_counters` |

## Operations and observation

| Native operation/callback | Local handle | Actor-safe owned operation | Semantics and consumer coverage |
| --- | --- | --- | --- |
| `label`, `ordered`, `maxRetransmitsOpt`, `maxPacketLifeTime`, `protocol`, `negotiated`, `id`, `priority` | `label()`, `configuration()` | `channel_label()`, `channel_configuration()` | Copy actual native values, including initially absent stream identity. Configuration is an independently sampled view. `remote_and_negotiated_channels_exchange_owned_bytes_deterministically`; `tests/production_session.rs`: `migrated_actor_is_woken_for_real_reliable_channel_delivery`. |
| `reliable` | `reliable()` | `channel_reliable()` | Native-derived reliability. `async_native_completion_size_and_counters`; actor migration test. |
| `state`, `error` | `state()`, `error()` | `channel_state()`, `channel_error()` | Native state and owned native closure error. Ordinary graceful close need not have an error. `close_and_drop_orders_quiesce_observers_and_reject_sends`. |
| `messages_sent`, `bytes_sent`, `messages_received`, `bytes_received` | `counters()` | `channel_counters()` | Cumulative native counters sampled independently, not Rust inbox-consumption counters or an atomic snapshot. `async_native_completion_size_and_counters`; actor migration test. |
| `buffered_amount`, `MaxSendQueueSize` | `buffered_amount()`, `send_queue_capacity()` | `channel_buffered_amount()`, `channel_send_queue_capacity()` | Native queued application bytes and send-queue capacity, not receive credit or negotiated message size. `advisory_notifications_coalesce_while_messages_remain_ordered`; actor migration test. |
| `Send` | `send()` | `send()` | Synchronous local admission with `DataChannelSendResult`; native boolean failures cannot supply the async error detail. `remote_and_negotiated_channels_exchange_owned_bytes_deterministically`. |
| `SendAsync` | `send_async()` | `send_async()` | Channel-local id and owned `SendComplete`. Reservation precedes the native call. At most 64 unconsumed/in-flight results; queue-capacity payload bounds fail before admission. Native error detail is preserved. Discarded callbacks settle `Canceled` without fabricating a native error. `async_native_completion_size_and_counters`, `async_admission_and_cancellation_survive_peer_drop`, actor migration test, `tests/controlled_driver.rs`: `two_peers_exchange_gathered_sdp_with_virtual_time_only`. |
| `Close` | `close()` | `close_channel()` | Initiates native graceful closure; retained send results survive close. Native failures can independently close a channel abruptly. `close_and_drop_orders_quiesce_observers_and_reject_sends`, async tests. |
| `RegisterObserver`, `UnregisterObserver` | `set_event_observation()` | `set_channel_event_observation()` | Registered initially; disable/re-enable does not detach send completion retention. Unregistration quiesces callbacks before observer destruction. `native_observer_registration_preserves_ordered_delivery`, async tests. |
| `OnStateChange` | `StateChanged` event | `try_channel_event()` | Latest callback-time snapshot, not state reconstructed during polling. Intermediate transitions coalesce. `advisory_notifications_coalesce_while_messages_remain_ordered`. |
| `OnMessage` | `Message` event | `try_channel_event()` | Owned binary/text bytes in FIFO delivery order. No fabricated application-controlled SCTP receive credit. Native pre-observer overflow is a native failure, not binding input rejection. Message retention is not bounded by advisory limits. Message exchange/overflow tests. |
| `OnBufferedAmountChange` | `BufferedAmountChanged` event | `try_channel_event()` | Accumulated removed bytes, saturating at `u64::MAX`, not remote acknowledgment. Advisory coalescing test. |
| `OnMaxMessageSize` | `MaxMessageSizeChanged` event | `try_channel_event()` | Latest native observer limit, independent of native send-queue capacity. Async size/error test. |
| `IsOkToCallOnTheNetworkThread` | Internal native observer uses network-thread delivery | Same observer implementation | Callbacks only copy/coalesce owned data and signal readiness; application code polls after stack unwinding. No mutable application session is reentered. Controlled execution uses the caller's driver, not autonomous tasks. Controlled driver and actor migration tests. |

The event queue prioritizes terminal send outcomes, then coalesced state,
buffered-amount and maximum-size advisories, then message deliveries. Each FIFO
queue preserves insertion order. Consuming a terminal closed snapshot discards
queued messages and suppresses later observer events, but does not discard send
outcomes. Dropping a retained channel ends application observability; callback
storage has no raw handle back-reference. Factory/thread ownership remains
retained through native handle teardown.
