"""Launch pinned build inputs with portable timeouts and native batch quoting."""

import argparse
import os
import signal
import subprocess
import sys


def batch_command(command: list[str]) -> str:
    # cmd /s /c strips its outer quotes. Pass the complete native command line
    # rather than applying CRT backslash escaping to the batch file's quotes.
    if not command or any(any(char in arg for char in '\r\n"%!') for arg in command):
        raise ValueError("batch arguments cannot contain quotes, newlines, % or !")
    executable = os.environ.get("COMSPEC", "cmd.exe")
    return f'"{executable}" /d /s /c "' + " ".join(f'"{arg}"' for arg in command) + '"'


def run(command: list[str], *, timeout: float | None = None, batch: bool = False,
        cwd: str | None = None) -> int:
    if timeout is not None and timeout <= 0:
        raise ValueError("timeout must be positive")
    if not command:
        raise ValueError("a command is required")
    native_command = batch_command(command) if batch else command
    with subprocess.Popen(native_command, cwd=cwd, start_new_session=os.name != "nt") as child:
        try:
            return child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            # Kill this launch's tree, not unrelated build or user processes.
            # A sync retry must not race a surviving git/CIPD download.
            if os.name == "nt":
                subprocess.run(["taskkill", "/f", "/t", "/pid", str(child.pid)],
                               check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                if child.poll() is None:
                    child.kill()
            else:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            child.wait()
            print(f"command timed out after {timeout:g}s: {command[0]}", file=sys.stderr)
            return 124


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--timeout", type=float)
    parser.add_argument("--batch", action="store_true")
    parser.add_argument("--cwd")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    try:
        return run(command, timeout=args.timeout, batch=args.batch, cwd=args.cwd)
    except (OSError, ValueError) as error:
        print(f"process launch: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
