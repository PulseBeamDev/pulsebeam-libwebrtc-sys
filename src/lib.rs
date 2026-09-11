//! Low-level Rust integration with PulseBeam's pinned libwebrtc artifact.

#[cxx::bridge(namespace = "pulsebeam::webrtc_sys")]
mod ffi {
    unsafe extern "C++" {
        include!("pulsebeam-webrtc-sys/native/probe.h");

        fn bridge_identity() -> &'static str;
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
