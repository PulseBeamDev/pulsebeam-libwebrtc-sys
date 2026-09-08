# PulseBeam libwebrtc build infrastructure

This repository pins and builds the complete static WebRTC archive used by
PulseBeam. It owns build tooling, ordered patches, and generated native
artifacts. Upstream owns WebRTC and its DEPS graph; consumer bindings and
applications live elsewhere.

## Prerequisites

Linux production builds require Git, Python 3, `just`, about 30 GB of free disk,
and network access to Chromium Git/CIPD/GCS services. Pinned depot_tools then
provides gclient, GN, Ninja, Chromium Clang, and the Debian 11 sysroot. The build
uses C++20 and the system libstdc++ ABI; archive bytes are not yet promised to be
reproducible across hosts.

## Commands

Run only the public interface, from any directory:

```sh
just -f /path/to/webrtc-build/Justfile bootstrap
just -f /path/to/webrtc-build/Justfile validate
just -f /path/to/webrtc-build/Justfile sync
just -f /path/to/webrtc-build/Justfile build linux-x86_64 production
just -f /path/to/webrtc-build/Justfile test-tooling
just -f /path/to/webrtc-build/Justfile check
```

`WEBRTC_WORK_ROOT`, `WEBRTC_CACHE_ROOT`, and `WEBRTC_OUTPUT_ROOT` override the
disposable checkout, content-addressed tool cache, and generated artifact root.
Defaults are `.work`, `.cache`, and `out` in this repository. `PYTHON` selects
the interpreter used by recipes.

The locked source identity is recorded in `upstream.lock.json` and has the form
`m<milestone>-<upstream-sha12>-pb.<recipe-sha12>`. A successful build writes
`out/<identity>/<target>/<profile>/obj/libwebrtc.a` and `build-record.json`,
which retains full source, dependency, and recipe identities plus exact GN
arguments. `patches/series` alone owns patch order; `patches/manifest.json`
binds every patch to an unpatched checkout identity and SHA-256.

Generated checkouts, caches, and outputs are disposable and ignored. Lock
refresh is never part of bootstrap, sync, or build.
