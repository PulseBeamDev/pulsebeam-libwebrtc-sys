//! One standalone application procedure, without a PeerConnectionFactory.
use pulsebeam_webrtc_sys::*;
use serde::Serialize;

#[path = "support/standalone_mixing.rs"]
mod mixing;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

struct Output(Arc<Mutex<Vec<u8>>>);
impl EventLogOutputSink for Output {
    fn is_active(&self) -> bool {
        true
    }
    fn write(&mut self, bytes: &[u8]) -> bool {
        self.0.lock().unwrap().extend_from_slice(bytes);
        true
    }
}

struct Source(Vec<i16>);
impl AudioMixerSourceProvider for Source {
    fn preferred_sample_rate(&mut self) -> Result<i32, AudioMixerError> {
        Ok(8000)
    }
    fn ssrc(&mut self) -> Result<i32, AudioMixerError> {
        Ok(42)
    }
    fn audio_frame(&mut self, rate: i32) -> Result<MixerAudioFrame, AudioMixerError> {
        assert_eq!(rate, 8000);
        let mut frame = AudioFrame::new();
        frame
            .update_frame(
                0,
                Some(&self.0),
                self.0.len(),
                rate,
                AudioFrameSpeechType::NORMAL_SPEECH,
                AudioFrameVadActivity::ACTIVE,
                1,
            )
            .unwrap();
        Ok(MixerAudioFrame::Normal(frame))
    }
}

#[derive(Debug, PartialEq, Serialize)]
struct Observation {
    processed: Vec<i16>,
    mixed: Vec<i16>,
    encoded: Vec<u8>,
    decoded: Vec<i16>,
    multi_source: mixing::Observation,
}

fn application<R: SessionRuntime>(runtime: R, mut progress: impl FnMut()) -> Observation {
    let mut factory = runtime.environment_factory().unwrap();
    factory
        .set_field_trials(FieldTrials::parse("StandaloneConstruction/Enabled/").unwrap())
        .unwrap();
    let environment = runtime.create_environment(&factory).unwrap();
    assert_eq!(
        environment.field_trials().lookup("StandaloneConstruction"),
        "Enabled"
    );
    // The native products retain the selected context, not this configuring owner.
    drop(factory);

    let mut processor =
        AudioProcessingModule::builtin(&environment, AudioProcessingConfig::default()).unwrap();
    assert_eq!(processor.initialize().unwrap(), AudioProcessingStatus::OK);
    let format = SdpAudioFormat::new(b"PCMU".to_vec(), 8000, 1);
    let encoder = AudioEncoderFactory::builtin()
        .unwrap()
        .create(
            &environment,
            &format,
            &AudioEncoderOptions {
                payload_type: 0,
                ..Default::default()
            },
        )
        .unwrap()
        .expect("native PCMU encoder");
    let decoder = AudioDecoderFactory::builtin()
        .unwrap()
        .create(&environment, &format)
        .unwrap()
        .expect("native PCMU decoder");
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let mut logger = RtcEventLogFactoryHandle::builtin()
        .create(&environment)
        .unwrap();
    logger
        .start(
            EventLogOutput::from_sink(Output(bytes.clone())),
            EventLogOutputPeriod::Immediate,
        )
        .unwrap();
    logger
        .log(RtcEvent::from_data(RtcEventData::AlrState { in_alr: true }).unwrap())
        .unwrap();
    drop(environment);

    let mut processed: Vec<i16> = (0..80).map(|i| (i * 173 - 6800) as i16).collect();
    assert_eq!(
        processor
            .process_interleaved(
                AudioProcessingStream {
                    sample_rate_hz: 8000,
                    channels: 1
                },
                &mut processed,
            )
            .unwrap(),
        AudioProcessingStatus::OK
    );
    let mut mixer = AudioMixer::stock().unwrap();
    let source = AudioMixerSource::from_provider(Source(processed.clone()));
    assert!(mixer.add_source(&source).unwrap());
    drop(source);
    let mixed = mixer.mix(1).unwrap().data_view().unwrap().to_vec();
    assert_eq!(mixed, processed);

    let mut encoded = Vec::new();
    let mut decoded = Vec::new();
    for block in 0..encoder.maximum_frames_in_packet() {
        let packet = encoder.encode((block * 80) as u32, &mixed).unwrap();
        if packet.bytes.is_empty() {
            continue;
        }
        for frame in decoder.parse_payload(packet.bytes.clone(), 0).unwrap() {
            decoded.extend(frame.frame.unwrap().decode().unwrap().unwrap().samples);
        }
        encoded.extend(packet.bytes);
    }
    assert!(!encoded.is_empty());
    assert!(!decoded.is_empty());
    assert!(decoded.iter().any(|&sample| sample != 0));
    // The stock SDK mixer is context-free. Keep its ordinary constructor,
    // inside the same generic native application rather than inventing an
    // environment dependency or a managed-only media procedure.
    let multi_source = mixing::run();
    let mut stop = logger.stop_async().unwrap();
    loop {
        if let Some(result) = stop.try_take() {
            result.unwrap();
            break;
        }
        progress();
    }
    drop(logger);
    assert!(!bytes.lock().unwrap().is_empty(), "actual stock log output");
    Observation {
        processed,
        mixed,
        encoded,
        decoded,
        multi_source,
    }
}

#[test]
fn same_standalone_procedure_uses_native_processing_mixing_codecs_and_logging() {
    let deadline = Instant::now() + Duration::from_secs(15);
    let real = application(ThreadedRuntime, || {
        assert!(Instant::now() < deadline, "threaded native stop stalled");
        std::thread::yield_now();
    });
    let mut observations = Vec::new();
    for _ in 0..2 {
        eprintln!("CONTROLLED_MEDIA_BEGIN");
        let world = ControlledWorld::acquire(1001, Duration::from_secs(7)).unwrap();
        let mut turns = 0;
        let managed = application(world.runtime().unwrap(), || {
            turns += 1;
            assert!(turns < 4096, "managed native stop stalled");
            assert!(!world.pump(64).blocked);
        });
        assert_eq!(managed, real);
        assert_eq!(world.now(), Duration::from_secs(7));
        // Retire the final context/products without advancing virtual time.
        drop(world);
        eprintln!("CONTROLLED_MEDIA_END");
        observations.push(managed);
    }
    if let Some(path) = std::env::var_os("PULSEBEAM_STANDALONE_MIXING_TRACE_PATH") {
        std::fs::write(path, serde_json::to_vec(&(&real, &observations)).unwrap()).unwrap();
    }
}

#[test]
fn failed_standalone_configuration_does_not_poison_runtime_authority() {
    let world = ControlledWorld::acquire(1002, Duration::from_secs(7)).unwrap();
    let runtime = world.runtime().unwrap();
    let mut closed = runtime.environment_factory().unwrap();
    closed.close();
    assert_eq!(
        runtime.create_environment(&closed).err(),
        Some(BuildEnvironmentError::Closed)
    );
    let factory = runtime.environment_factory().unwrap();
    let environment = runtime.create_environment(&factory).unwrap();
    assert_eq!(environment.now(), world.now());
    drop(environment);
    drop(factory);
    drop(runtime);
    drop(world);
    let next = ControlledWorld::acquire(1003, Duration::from_secs(9)).unwrap();
    assert_eq!(next.now(), Duration::from_secs(9));
}
