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
    "pi_dirs",
}

DEFAULT_SUBPATHS = {
    "claude_project_dirs": ".claude/projects",
    "codex_sessions_dirs": ".codex/sessions",
    "opencode_dirs": ".local/share/opencode",
    "gemini_dirs": ".gemini",
    "pi_dirs": ".pi/agent/sessions",
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


class ConfigError(Exception):
    """The existing config has a managed key with a value we won't silently
    mangle (e.g. a scalar where an array is expected). Surfaced as exit 3,
    the same code read_config uses for a parse failure, so the caller treats
    it as a config error to fix rather than a change to apply."""


def apply_agents(config: dict, agents: list[tuple[str, str]], home: str) -> dict:
    """Merge agent mirror paths into config dict. Returns updated dict.

    Raises ConfigError if a managed key holds a non-list value. A
    hand-written scalar like ``claude_project_dirs = "/some/path"`` parses
    fine as TOML, and ``list()`` on a string character-splits it into
    ``['/', 's', 'o', ...]`` -- a silent, destructive rewrite. Reject it
    instead, the way read_config already rejects a parse failure.
    """
    result = dict(config)
    for key, mirror_path in agents:
        if key in result:
            existing = result[key]
            if not isinstance(existing, list):
                raise ConfigError(
                    f"{key} must be an array of paths, got "
                    f"{type(existing).__name__}: {existing!r}"
                )
            # Copy so we never mutate the caller's dict in place.
            existing = list(existing)
        else:
            # Key absent: agentsview would use its built-in default. Materialize
            # that default explicitly so adding the mirror doesn't replace it.
            existing = [default_host_path(key, home)]
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

        # Test 1: Missing config file -> creates all keys [default, mirror]
        name = "missing config -> all keys"
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

        # Test 2: Existing file with empty dict -> adds all keys
        name = "empty file -> all keys"
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

        # Test 4: User-customized array (without default) is preserved as-is;
        # only the mirror is appended. The default is NOT re-injected, because
        # the user's explicit array represents an intentional customization.
        name = "user customizations not overridden"
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
            if val != [user_entry, mirror]:
                fail(name, f"expected [user, mirror], got: {val}")
            elif expected_default in val:
                fail(name, f"default unexpectedly re-injected: {val}")
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

        # Test 7: A scalar value for a managed key raises ConfigError instead
        # of being character-split by list(). Pre-existing bug, only reachable
        # through the one-time opt-in today; re-syncing on every run makes it
        # reachable every run, so it must fail loudly instead of silently
        # turning "/some/path" into ['/', 's', 'o', ...].
        name = "scalar managed key raises ConfigError"
        try:
            key = "claude_project_dirs"
            cfg = {key: "/some/scalar/path"}
            raised = False
            try:
                apply_agents(cfg, [(key, "/mnt/mirror/claude_project_dirs")], home)
            except ConfigError:
                raised = True
            if raised:
                ok(name)
            else:
                fail(name, "ConfigError", "no exception (value would be char-split)")
        except Exception:
            fail(name, traceback.format_exc())

        # Test 8: --check semantics when nothing is pending. apply_agents is a
        # no-op (mirror already present), so updated == config -> exit 0. This
        # is the case --diff gets wrong on a hand-edited file: comments and
        # formatting drift make the rendered diff non-empty forever, which
        # would re-prompt every run even though nothing is actually missing.
        name = "check: no pending changes (parsed equality)"
        try:
            key = "claude_project_dirs"
            mirror = "/mnt/mirror/claude_project_dirs"
            existing_default = default_host_path(key, home)
            cfg = {key: [existing_default, mirror]}
            updated = apply_agents(cfg, [(key, mirror)], home)
            if updated == cfg:
                ok(name)
            else:
                fail(name, "apply_agents(config) == config", f"got {updated}")
        except Exception:
            fail(name, traceback.format_exc())

        # Test 9: --check semantics when a mirror path is missing. updated !=
        # config -> exit 1. A missing mirror is the only condition that should
        # produce a prompt; comment/formatting drift must not.
        name = "check: pending changes detected (mirror missing)"
        try:
            key = "claude_project_dirs"
            mirror = "/mnt/mirror/claude_project_dirs"
            existing_default = default_host_path(key, home)
            cfg = {key: [existing_default]}  # mirror absent
            updated = apply_agents(cfg, [(key, mirror)], home)
            if updated != cfg:
                ok(name)
            else:
                fail(name, "apply_agents(config) != config",
                     "equal (missing mirror not detected)")
        except Exception:
            fail(name, traceback.format_exc())

    if failures:
        for msg in failures:
            print(msg, file=sys.stderr)
        sys.exit(1)
    print("All 9 self-tests passed.")


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
    mode.add_argument("--check", action="store_true",
                      help="Exit 0 if no changes are needed, 1 if a mirror path "
                           "is still missing, 3 on config error (no write)")
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

    try:
        config = read_config(args.config_path)
        updated = apply_agents(config, agents, args.home)
    except ConfigError as e:
        print(f"error: {args.config_path}: {e}", file=sys.stderr)
        sys.exit(3)

    if args.check:
        # Compare parsed values, not rendered text. The writer round-trips
        # through tomllib/tomli_w, so it drops comments and normalizes
        # formatting; a --diff against a hand-edited file is non-empty forever,
        # which would re-prompt on every run even when every mirror path is
        # already present. Comparing the merged dict to the original is immune
        # to that: exit 0 when the merge is a no-op, 1 when a mirror path is
        # still missing. (ConfigError is raised inside apply_agents above, so a
        # scalar managed key exits 3 here before this branch.)
        sys.exit(0 if updated == config else 1)

    new_content = dumps(updated)

    if args.diff:
        do_diff(args.config_path, new_content)
    elif args.write:
        do_write(args.config_path, new_content)


if __name__ == "__main__":
    main()
