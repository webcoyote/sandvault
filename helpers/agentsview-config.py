#!/usr/bin/env python3
"""Update ~/.agentsview/config.toml with sandvault session mirror paths."""

from __future__ import annotations

import argparse
import difflib
import os
import sys
import tempfile
from pathlib import Path

# Sibling vendored libraries
sys.path.insert(0, str(Path(__file__).parent))
from tomli_w import dumps

try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:
    from tomli import load as _tomli_load, loads as _tomli_loads

    class tomllib:
        load = staticmethod(_tomli_load)
        loads = staticmethod(_tomli_loads)

VALID_KEYS = {
    "claude_project_dirs",
    "codex_sessions_dirs",
    "opencode_dirs",
    "gemini_dirs",
}

DEFAULT_SUBPATHS = {
    "claude_project_dirs": ".claude/projects",
    "codex_sessions_dirs": ".codex/sessions",
    "opencode_dirs": ".local/share/opencode",
    "gemini_dirs": ".gemini",
}


def default_host_path(key: str, home: str) -> str:
    return str(Path(home) / DEFAULT_SUBPATHS[key])


def read_config(config_path: str) -> dict:
    p = Path(config_path)
    if not p.exists():
        return {}
    try:
        with open(p, "rb") as f:
            return tomllib.load(f)
    except tomllib.TOMLDecodeError as e:
        print(f"error: cannot parse {config_path}: {e}", file=sys.stderr)
        sys.exit(3)


def apply_agents(config: dict, agents: list[tuple[str, str]], home: str) -> dict:
    """Merge agent mirror paths into config dict. Returns updated dict."""
    result = dict(config)
    for key, mirror_path in agents:
        default = default_host_path(key, home)
        existing = list(result.get(key, []))
        # Append default if absent
        if default not in existing:
            existing.append(default)
        # Append mirror if absent
        if mirror_path not in existing:
            existing.append(mirror_path)
        result[key] = existing
    return result


def do_diff(config_path: str, new_content: str) -> None:
    p = Path(config_path)
    old_lines = p.read_text().splitlines(keepends=True) if p.exists() else []
    new_lines = new_content.splitlines(keepends=True)
    diff = list(difflib.unified_diff(
        old_lines,
        new_lines,
        fromfile=config_path,
        tofile=config_path + " (proposed)",
    ))
    if diff:
        sys.stdout.writelines(diff)
    else:
        print("(no changes)")


def do_write(config_path: str, new_content: str) -> None:
    p = Path(config_path)
    parent = p.parent
    if not parent.exists():
        parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    tmp = str(p) + ".tmp"
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    try:
        os.write(fd, new_content.encode())
    finally:
        os.close(fd)
    os.replace(tmp, str(p))


def run_self_test() -> None:
    import traceback

    failures: list[str] = []

    def fail(name: str, msg: str) -> None:
        failures.append(f"FAIL [{name}]: {msg}")

    def ok(name: str) -> None:
        print(f"ok  {name}")

    with tempfile.TemporaryDirectory() as tmp:
        home = os.path.join(tmp, "home")
        os.makedirs(home, mode=0o700)
        config_dir = os.path.join(home, ".agentsview")
        config_path = os.path.join(config_dir, "config.toml")

        all_agents = [(k, f"/mnt/mirror/{k}") for k in VALID_KEYS]

        # Test 1: Missing config file -> creates all four keys [default, mirror]
        name = "missing config -> all four keys"
        try:
            cfg = read_config(config_path)
            updated = apply_agents(cfg, all_agents, home)
            for key, mirror in all_agents:
                expected_default = default_host_path(key, home)
                if key not in updated:
                    fail(name, f"key {key} missing")
                    break
                if expected_default not in updated[key]:
                    fail(name, f"default path missing from {key}")
                    break
                if mirror not in updated[key]:
                    fail(name, f"mirror path missing from {key}")
                    break
                if updated[key] != [expected_default, mirror]:
                    fail(name, f"{key} has unexpected value: {updated[key]}")
                    break
            else:
                ok(name)
        except Exception:
            fail(name, traceback.format_exc())

        # Test 2: Existing file with empty dict -> adds all four keys
        name = "empty file -> all four keys"
        try:
            os.makedirs(config_dir, exist_ok=True)
            with open(config_path, "w") as f:
                f.write("")
            cfg = read_config(config_path)
            updated = apply_agents(cfg, all_agents, home)
            for key, mirror in all_agents:
                expected_default = default_host_path(key, home)
                if updated.get(key) != [expected_default, mirror]:
                    fail(name, f"{key} unexpected: {updated.get(key)}")
                    break
            else:
                ok(name)
        except Exception:
            fail(name, traceback.format_exc())

        # Test 3: Key already contains mirror path -> no duplicate
        name = "key already has mirror -> no duplicate"
        try:
            key = "claude_project_dirs"
            mirror = "/mnt/mirror/claude_project_dirs"
            existing_default = default_host_path(key, home)
            initial = {key: [existing_default, mirror]}
            with open(config_path, "wb") as f:
                f.write(dumps(initial).encode())
            cfg = read_config(config_path)
            updated = apply_agents(cfg, [(key, mirror)], home)
            val = updated[key]
            if val.count(mirror) != 1:
                fail(name, f"mirror appears {val.count(mirror)} times: {val}")
            elif val.count(existing_default) != 1:
                fail(name, f"default appears {val.count(existing_default)} times: {val}")
            else:
                ok(name)
        except Exception:
            fail(name, traceback.format_exc())

        # Test 4: User-added entries are preserved, default+mirror appended if absent
        name = "user entries preserved"
        try:
            key = "claude_project_dirs"
            mirror = "/mnt/mirror/claude_project_dirs"
            user_entry = "/home/user/custom-claude"
            initial = {key: [user_entry]}
            with open(config_path, "wb") as f:
                f.write(dumps(initial).encode())
            cfg = read_config(config_path)
            updated = apply_agents(cfg, [(key, mirror)], home)
            val = updated[key]
            expected_default = default_host_path(key, home)
            if user_entry not in val:
                fail(name, f"user entry missing: {val}")
            elif expected_default not in val:
                fail(name, f"default missing: {val}")
            elif mirror not in val:
                fail(name, f"mirror missing: {val}")
            elif val[0] != user_entry:
                fail(name, f"user entry not preserved at front: {val}")
            else:
                ok(name)
        except Exception:
            fail(name, traceback.format_exc())

        # Test 5: Unrelated top-level keys preserved after write
        name = "unrelated keys preserved"
        try:
            initial = {
                "host": "x",
                "port": 8080,
                "claude_project_dirs": ["/some/path"],
            }
            with open(config_path, "wb") as f:
                f.write(dumps(initial).encode())
            cfg = read_config(config_path)
            mirror = "/mnt/mirror/claude_project_dirs"
            updated = apply_agents(cfg, [("claude_project_dirs", mirror)], home)
            new_content = dumps(updated)
            do_write(config_path, new_content)
            with open(config_path, "rb") as f:
                written = tomllib.load(f)
            if written.get("host") != "x":
                fail(name, f"host lost: {written}")
            elif written.get("port") != 8080:
                fail(name, f"port lost: {written}")
            else:
                ok(name)
        except Exception:
            fail(name, traceback.format_exc())

        # Test 6: Atomic write - simulate crash by patching os.replace
        name = "atomic write: crash leaves original intact"
        try:
            original_content = "host = \"original\"\n"
            with open(config_path, "w") as f:
                f.write(original_content)

            import unittest.mock as mock

            def crashing_replace(src, dst):
                raise OSError("simulated crash")

            with mock.patch("os.replace", side_effect=crashing_replace):
                try:
                    do_write(config_path, "host = \"new\"\n")
                except OSError:
                    pass

            with open(config_path) as f:
                actual = f.read()
            if actual != original_content:
                fail(name, f"original file changed: {repr(actual)}")
            else:
                ok(name)
        except Exception:
            fail(name, traceback.format_exc())

    if failures:
        for msg in failures:
            print(msg, file=sys.stderr)
        sys.exit(1)
    print(f"All {6} self-tests passed.")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Update agentsview config.toml with sandvault mirror paths."
    )
    parser.add_argument("--config-path", metavar="PATH",
                        help="Path to agentsview config.toml")
    parser.add_argument("--home", metavar="PATH",
                        help="Host user home directory")
    parser.add_argument("--agent", metavar="KEY=MIRROR_PATH", action="append",
                        default=[], dest="agents",
                        help="Agent key and mirror path (repeatable)")

    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--diff", action="store_true",
                      help="Print unified diff of proposed changes")
    mode.add_argument("--write", action="store_true",
                      help="Write updated config atomically")
    mode.add_argument("--self-test", action="store_true",
                      help="Run in-process self-tests and exit")

    args = parser.parse_args()

    if args.self_test:
        run_self_test()
        return

    if not args.config_path:
        parser.error("--config-path is required")
    if not args.home:
        parser.error("--home is required")

    # Parse --agent KEY=MIRROR_PATH pairs
    agents: list[tuple[str, str]] = []
    for spec in args.agents:
        if "=" not in spec:
            print(f"error: --agent value must be KEY=MIRROR_PATH, got: {spec!r}",
                  file=sys.stderr)
            sys.exit(2)
        key, _, mirror_path = spec.partition("=")
        if key not in VALID_KEYS:
            print(
                f"error: unknown agent key {key!r}. "
                f"Valid keys: {', '.join(sorted(VALID_KEYS))}",
                file=sys.stderr,
            )
            sys.exit(2)
        agents.append((key, mirror_path))

    config = read_config(args.config_path)
    updated = apply_agents(config, agents, args.home)
    new_content = dumps(updated)

    if args.diff:
        do_diff(args.config_path, new_content)
    elif args.write:
        do_write(args.config_path, new_content)


if __name__ == "__main__":
    main()
