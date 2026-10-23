"""Synthetic manifest inputs for tests of the current source tree.

Historical fixture hashes describe historical bytes, not today's build. Real
artifacts must always be checked unchanged by the production validators.
"""

import hashlib
import json
from pathlib import Path
import shlex
import sys

from tools.native_provenance import adapter_digest, configuration_digest


ROOT = Path(__file__).resolve().parents[1]


def current_manifest(root: Path = ROOT) -> dict:
    manifest = json.loads((ROOT / "tests/fixtures/artifact-support-manifest.json").read_text())
    config = manifest["native_configuration"]
    manifest["bridge"]["native_adapter_sha256"] = config["native_adapter_sha256"] = adapter_digest(root)
    wrapper = root / "tools/native_cc_wrapper.py"
    wrapper_digest = hashlib.sha256(wrapper.read_bytes()).hexdigest()
    config["floating_point"]["compiler_wrapper_sha256"] = wrapper_digest
    config["gn_args"]["cc_wrapper"] = shlex.join([
        sys.executable, str(wrapper), "--contract-sha256", wrapper_digest, "sccache",
    ])
    manifest["native_configuration_sha256"] = configuration_digest(config)
    return manifest
