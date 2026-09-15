Status: COMPLETED

Implemented:
- Propagated `-fsanitize=address` to every manual bridge C++ invocation and to C++ smoke compilation/link when the sanitizer producer mode is selected.
- Added executable compiler-stub coverage for both Linux x86_64 flavors, all nine bridge translation units, C++ smoke, retained Rust sanitizer boundaries, and ordinary-mode isolation.

Verification:
- `python3 -m unittest -v tests/test_linux_asan_configuration.py tests/test_linux_asan_static_closure.py` (11 passed)
- `just check` (passed)
- `PULSEBEAM_WEBRTC_SANITIZER=address ASAN_OPTIONS=detect_leaks=1:halt_on_error=1:strict_string_checks=1 just build core linux-x86_64` (passed)
- `PULSEBEAM_WEBRTC_SANITIZER=address ASAN_OPTIONS=detect_leaks=1:halt_on_error=1:strict_string_checks=1 just build native linux-x86_64` (passed)
- `git diff --check a11e59c057efb3bdf445d86af6b5d307581cbe18..HEAD` (pending candidate commit)

Files:
- `Justfile`
- `tests/test_linux_asan_configuration.py`
- `plans/linux-release-completion/work/task-2-build-report.md`

Notes:
- RED was the compiler-stub regression: all nine manual bridge commands and C++ smoke omitted `-fsanitize=address` before the implementation.
