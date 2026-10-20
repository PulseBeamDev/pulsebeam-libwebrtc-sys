//! Producer-owned, device-free mobile acceptance entry points.
use std::ffi::c_void;

#[allow(dead_code)]
#[path = "../../rust-only/src/main.rs"]
mod scenario;

unsafe extern "C" {
    fn pulsebeam_sdk_smoke() -> i32;
}

#[unsafe(no_mangle)]
pub extern "C" fn pulsebeam_headless_run() -> i32 {
    match std::panic::catch_unwind(|| {
        scenario::run();
        // This companion is compiled against the exported public SDK headers
        // and links the same archive as the Rust bridge, not substitute codecs.
        assert_eq!(unsafe { pulsebeam_sdk_smoke() }, 0);
    }) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_pulsebeam_Headless_runNative(
    _environment: *mut c_void,
    _class: *mut c_void,
) -> i32 {
    pulsebeam_headless_run()
}
