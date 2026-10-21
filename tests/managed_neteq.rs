//! Actual native PCMU packets through stock NetEq using the selected runtime clock.
use pulsebeam_webrtc_sys::*;
use serde::Serialize;
use std::{fmt::Debug, time::Duration};

fn serialize_debug<T: Debug, S: serde::Serializer>(
    value: &T,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("{value:?}"))
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct State {
    #[serde(serialize_with = "serialize_debug")]
    network: NetEqNetworkStatistics,
    #[serde(serialize_with = "serialize_debug")]
    lifetime: NetEqLifetimeStatistics,
    #[serde(serialize_with = "serialize_debug")]
    operations: NetEqOperationsAndState,
    #[serde(serialize_with = "serialize_debug")]
    decoder: Option<NetEqDecoderFormat>,
    playout_timestamp: Option<u32>,
    target_delay_ms: i32,
    filtered_current_delay_ms: i32,
    sync_buffer_size_ms: i32,
    last_output_sample_rate_hz: i32,
}
impl State {
    fn take(engine: &mut NetEq) -> Self {
        Self {
            network: engine.current_network_statistics(),
            lifetime: engine.lifetime_statistics(),
            operations: engine.operations_and_state(),
            decoder: engine.current_decoder_format(),
            playout_timestamp: engine.playout_timestamp(),
            target_delay_ms: engine.target_delay_ms(),
            filtered_current_delay_ms: engine.filtered_current_delay_ms(),
            sync_buffer_size_ms: engine.sync_buffer_size_ms(),
            last_output_sample_rate_hz: engine.last_output_sample_rate_hz(),
        }
    }
}

#[derive(Debug, PartialEq, Serialize)]
struct Insertion {
    #[serde(serialize_with = "serialize_debug")]
    header: RtpHeaderValues,
    receive_time_us: i64,
    code: i32,
}

#[derive(Debug, PartialEq, Serialize)]
struct Step {
    input_timestamp: u32,
    input: Vec<i16>,
    #[serde(serialize_with = "serialize_debug")]
    encoded: AudioEncoderOutput,
    insertion: Option<Insertion>,
    code: i32,
    muted: bool,
    output_rate_hz: i32,
    #[serde(serialize_with = "serialize_debug")]
    fields: AudioFrameFields,
    channel_layout: i32,
    absolute_capture_timestamp_ms: Option<i64>,
    samples: Vec<i16>,
    state: State,
}

#[derive(Debug, PartialEq, Serialize)]
struct Observation {
    clock_bounds_us: (i64, i64),
    started_at_us: i64,
    finished_at_us: i64,
    before: State,
    steps: Vec<Step>,
    final_network_code: i32,
    #[serde(serialize_with = "serialize_debug")]
    final_network: NetEqNetworkStatistics,
    after_reset: State,
}

fn application<R: SessionRuntime>(runtime: R) -> Observation {
    let factory = runtime.environment_factory().unwrap();
    let environment = runtime.create_environment(&factory).unwrap();
    let format = SdpAudioFormat::new(b"PCMU".to_vec(), 8000, 1);
    let encoders = AudioEncoderFactory::builtin().unwrap();
    let decoders = AudioDecoderFactory::builtin().unwrap();
    let encoder = encoders
        .create(
            &environment,
            &format,
            &AudioEncoderOptions {
                payload_type: 0,
                codec_pair_id: None,
            },
        )
        .unwrap()
        .expect("actual native PCMU encoder");
    let mut config = NetEqConfig::default();
    config.sample_rate_hz = 8000;
    let mut engine = NetEq::stock(&environment, &config, &decoders).unwrap();
    assert!(engine.register_payload_type(0, &format));
    let bounds = engine.clock_time_bounds();
    assert_eq!(bounds, environment.clock_time_bounds());
    drop(factory);
    drop(encoders);
    drop(decoders);
    // Keep this exact selected environment to sample arrival time, not another
    // clock or a normalized logical timestamp. Products retain their own copies.
    let started_at_us = environment.timestamp().microseconds().unwrap();
    let before = State::take(&mut engine);
    let mut steps = Vec::new();
    let mut sequence = u16::MAX - 4;
    for block in 0..40 {
        let input_timestamp = 1000 + block * 80;
        let input = (0..80)
            .map(|i| {
                if (block * 80 + i) % 8 < 4 {
                    12000
                } else {
                    -12000
                }
            })
            .collect::<Vec<_>>();
        let encoded = encoder.encode(input_timestamp, &input).unwrap();
        let insertion = if encoded.bytes.is_empty() {
            None
        } else {
            let header = RtpHeaderValues {
                payload_type: 0,
                sequence_number: sequence,
                timestamp: encoded.info.primary.encoded_timestamp,
                ssrc: 73,
                ..Default::default()
            };
            let receive_time = environment.timestamp();
            assert!(
                engine.receive_time_is_safe(receive_time),
                "retained bounds must prove finite subtraction"
            );
            let code = engine
                .insert_packet_at(&header, &encoded.bytes, receive_time)
                .unwrap();
            assert_eq!(code, NetEqReturnCode::OK);
            sequence = sequence.wrapping_add(1);
            Some(Insertion {
                header,
                receive_time_us: receive_time.microseconds().unwrap(),
                code: code.0,
            })
        };
        let mut frame = AudioFrame::new();
        let mut muted = true;
        let mut output_rate_hz = -1;
        let code = engine
            .get_audio(
                &mut frame,
                Some(&mut muted),
                Some(&mut output_rate_hz),
                None,
            )
            .unwrap();
        assert_eq!(code, NetEqReturnCode::OK);
        let fields = frame.fields();
        assert_eq!(
            (
                fields.sample_rate_hz,
                fields.num_channels,
                fields.samples_per_channel
            ),
            (8000, 1, 80)
        );
        let samples = frame.data_view().unwrap().to_vec();
        steps.push(Step {
            input_timestamp,
            input,
            encoded,
            insertion,
            code: code.0,
            muted,
            output_rate_hz,
            fields,
            channel_layout: frame.channel_layout().0,
            absolute_capture_timestamp_ms: frame.absolute_capture_timestamp_ms(),
            samples,
            state: State::take(&mut engine),
        });
    }
    let (final_network_code, final_network) = engine.network_statistics();
    assert_eq!(final_network_code, NetEqReturnCode::OK);
    let after_reset = State::take(&mut engine);
    assert!(after_reset.lifetime.total_samples_received >= 1600);
    assert!(after_reset.lifetime.jitter_buffer_emitted_count > 0);
    assert!(after_reset.playout_timestamp.is_some());
    assert!(
        steps
            .iter()
            .any(|s| s.state.operations.current_buffer_size_ms > 0)
    );
    assert!(
        steps
            .iter()
            .filter(|s| s.samples.iter().any(|&v| v != 0))
            .count()
            >= 20
    );
    assert!(
        steps.iter().any(|s| !s.fields.packet_infos.is_empty()),
        "native finite arrival metadata must reach output frames"
    );
    drop(encoder);
    drop(engine);
    let finished_at_us = environment.timestamp().microseconds().unwrap();
    drop(environment);
    Observation {
        clock_bounds_us: (
            bounds.minimum().microseconds(),
            bounds.maximum().microseconds(),
        ),
        started_at_us,
        finished_at_us,
        before,
        steps,
        final_network_code: final_network_code.0,
        final_network,
        after_reset,
    }
}

#[test]
fn native_pcmu_packets_play_through_stock_neteq_and_replay() {
    let threaded = application(ThreadedRuntime);
    let managed = std::array::from_fn::<_, 2, _>(|_| {
        eprintln!("CONTROLLED_MEDIA_BEGIN");
        let world = ControlledWorld::acquire(1005, Duration::from_secs(7)).unwrap();
        let observation = application(world.runtime().unwrap());
        assert_eq!(world.now(), Duration::from_secs(7));
        drop(world);
        eprintln!("CONTROLLED_MEDIA_END");
        observation
    });
    if let Some(path) = std::env::var_os("PULSEBEAM_NETEQ_TRACE_PATH") {
        std::fs::write(path, serde_json::to_vec(&(&threaded, &managed)).unwrap()).unwrap();
    }
    // Same complete clock/arrival inputs prove full managed replay, including
    // every native statistic and received-packet field, not just decoded PCM.
    assert_eq!(managed[0], managed[1]);
    for observation in &managed {
        assert_eq!(observation.started_at_us, 7_000_000);
        assert_eq!(observation.finished_at_us, 7_000_000);
        for (real, controlled) in threaded.steps.iter().zip(&observation.steps) {
            assert_eq!(real.encoded, controlled.encoded);
            assert_eq!(real.samples, controlled.samples);
        }
    }
    // Default threaded arrivals use real environment time, not the managed
    // 7s clock. Retain all of those timestamps and processing-delay statistics;
    // claiming full cross-runtime timing equality would violate preconditions.
}
