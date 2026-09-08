set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

root := justfile_directory()
python := env_var_or_default("PYTHON", "python3")

bootstrap:
    cd "{{root}}" && "{{python}}" tools/webrtc.py bootstrap

validate:
    cd "{{root}}" && "{{python}}" tools/webrtc.py validate

sync:
    cd "{{root}}" && "{{python}}" tools/webrtc.py sync

build target profile:
    cd "{{root}}" && "{{python}}" tools/webrtc.py build "{{target}}" "{{profile}}"

test-tooling:
    cd "{{root}}" && PYTHONPATH="{{root}}/tools" "{{python}}" -m unittest discover -s tests -v

check: validate test-tooling
    cd "{{root}}" && git diff --check
