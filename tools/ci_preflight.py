"""Check release CLI options against the installed gh before expensive jobs.

Run locally with `just ci-preflight`. No GitHub authentication or network is needed.
"""

from __future__ import annotations

import ast
import re
import subprocess
import sys
from pathlib import Path

from tools import github_release

WORKFLOWS = Path(__file__).resolve().parents[1] / ".github" / "workflows"
HELP_OPTION = re.compile(r"(?m)^\s+(?:-\w,\s+)?(--[a-z][a-z0-9-]*)\b")


def release_options_in_source() -> dict[str, set[str]]:
    """Read literal gh options from actual calls, not only the preflight list."""
    source = Path(github_release.__file__).read_text(encoding="utf-8")
    used: dict[str, set[str]] = {}
    for node in ast.walk(ast.parse(source)):
        if not isinstance(node, ast.Call) or not isinstance(node.func, ast.Name) or node.func.id != "gh":
            continue
        literals = [arg.value if isinstance(arg, ast.Constant) and isinstance(arg.value, str) else None for arg in node.args]
        if not literals or literals[0] not in {"api", "release"}:
            continue
        command = "api" if literals[0] == "api" else f"release {literals[1]}"
        used.setdefault(command, set()).update(value.split("=", 1)[0] for value in literals if value and value.startswith("--"))
    return used


def check_release_options(workflows: Path = WORKFLOWS) -> list[str]:
    """Return diagnostics for unsupported gh options or inline release scripts."""
    errors: list[str] = []
    for path in sorted(workflows.glob("*.yml")):
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if "gh release " in line and not line.lstrip().startswith("#"):
                errors.append(f"{path}:{number}: move gh release scripting behind just ci-release")
    actual = release_options_in_source()
    if actual != github_release.GH_OPTIONS:
        errors.append(f"release CLI preflight flags differ from actual gh calls: declared={github_release.GH_OPTIONS}, used={actual}")
    for command, options in actual.items():
        result = subprocess.run(["gh", *command.split(), "--help"], text=True, capture_output=True, check=False)
        if result.returncode:
            errors.append(f"gh {command} --help failed: {result.stderr.strip()}")
            continue
        supported = set(HELP_OPTION.findall(result.stdout))
        for option in sorted(options - supported):
            errors.append(f"gh {command} does not support {option}")
    return errors


def main() -> int:
    try:
        errors = check_release_options()
    except (OSError, subprocess.SubprocessError) as exc:
        print(f"CI preflight could not check gh: {exc}", file=sys.stderr)
        return 1
    for error in errors:
        print(error, file=sys.stderr)
    if errors:
        return 1
    print("CI release CLI options are supported by the installed gh")
    return 0


if __name__ == "__main__":
    sys.exit(main())
