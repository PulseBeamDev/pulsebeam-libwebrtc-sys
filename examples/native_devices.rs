//! Inspect native Linux devices without acquiring a camera or requesting portal consent.
//! Set PULSEBEAM_CAMERA_ID or PULSEBEAM_SCREEN_ID explicitly to attempt capture;
//! PULSEBEAM_AUDIO=1 opts in to the platform microphone/speaker ADM.

#[cfg(not(feature = "native"))]
fn main() {
    eprintln!("run with --features native and a matching native artifact");
}

#[cfg(feature = "native")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Duration;

    use pulsebeam_webrtc_sys::PeerConnectionFactory;

    let factory = PeerConnectionFactory::builder().build()?;
    match factory.camera_devices() {
        Ok(devices) => {
            for device in &devices {
                println!("camera: {} ({})", device.name, device.id);
            }
        }
        Err(error) => eprintln!("camera discovery unavailable: {error}"),
    }
    match factory.screen_sources() {
        Ok(sources) => {
            for source in sources {
                println!("screen: {} ({})", source.name, source.id);
            }
        }
        Err(error) => eprintln!("desktop discovery unavailable: {error}"),
    }

    if let Ok(id) = std::env::var("PULSEBEAM_CAMERA_ID") {
        let format = factory
            .camera_formats(&id)?
            .into_iter()
            .next()
            .ok_or("camera has no usable capture formats")?;
        let mut camera = factory.open_camera(&id, format.width, format.height, format.max_fps)?;
        println!("camera state: {:?}", camera.status(Duration::from_secs(2)));
        camera.stop()?;
    }
    if let Ok(id) = std::env::var("PULSEBEAM_SCREEN_ID") {
        let mut screen = factory.open_screen(id.parse()?)?;
        // The portal owns any Wayland consent UI. A successful open can still
        // be pending selection and is not proof that a frame was delivered.
        println!("screen state: {:?}", screen.status());
        screen.stop()?;
    }
    if std::env::var("PULSEBEAM_AUDIO").as_deref() == Ok("1") {
        match PeerConnectionFactory::builder().native_audio(true).build() {
            Ok(audio) => {
                for recording in [true, false] {
                    match audio.audio_devices(recording) {
                        Ok(devices) => println!("recording={recording} devices: {devices:?}"),
                        Err(error) => eprintln!("audio discovery failed: {error}"),
                    }
                }
                // Select a current device index before creating a microphone
                // track or negotiating streams. Playout is controlled by a
                // peer's set_native_audio_enabled(false, enabled) method.
            }
            Err(error) => eprintln!("native audio unavailable: {error}"),
        }
    }
    Ok(())
}
