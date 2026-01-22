"""Tests for pre_tool_use.py security hook."""
import importlib

import pytest
from pre_tool_use import (
    is_dangerous_rm_command,
    is_env_file_access,
)


class TestDangerousRmDetection:
    """Tests for dangerous rm command detection."""

    @pytest.mark.parametrize("command", [
        "rm -rf /",
        "rm -rf /*",
        "rm -rf ~",
        "rm -rf ~/",
        "rm -rf $HOME",
        "rm -rf .",
        "rm -rf ..",
        "rm -rf *",
        "rm -Rf /tmp",
        "rm -fr /var",
        "rm --recursive --force /",
        "rm --force --recursive /home",
        "rm -r -f /",
        "rm -f -r /",
        "sudo rm -rf /",
        "  rm   -rf   /  ",
        "rm -rfv /",
    ])
    def test_dangerous_commands_blocked(self, command):
        """Verify dangerous rm commands are detected."""
        assert is_dangerous_rm_command(command) is True, f"Should block: {command}"

    @pytest.mark.parametrize("command", [
        "rm file.txt",
        "rm -f file.txt",
        "rm -f /tmp/specific-file.txt",
        "rm single_file",
        "ls -la",
        "cat /etc/passwd",
        "mkdir -p /tmp/test",
    ])
    def test_safe_commands_allowed(self, command):
        """Verify safe commands are not blocked."""
        assert is_dangerous_rm_command(command) is False, f"Should allow: {command}"

    @pytest.mark.parametrize("command", [
        # These are intentionally blocked by the conservative security policy
        "rm -r ./node_modules",  # ./ contains . which matches dangerous path pattern
        "rm -rf ./build",        # Same as above
        "echo rm -rf /",         # Contains rm -rf / pattern (no shell parsing)
    ])
    def test_conservative_blocks(self, command):
        """Verify conservative security policy blocks edge cases.

        The security function is intentionally conservative and will block
        commands that contain dangerous patterns even in safe contexts.
        This is by design to prevent false negatives.
        """
        assert is_dangerous_rm_command(command) is True, f"Should block (conservative): {command}"


class TestExternalDriveAccess:
    """Tests for external drive access blocking."""

    @pytest.mark.parametrize("tool_name,tool_input", [
        ("Read", {"file_path": "/Volumes/ExternalDrive/file.txt"}),
        ("Write", {"file_path": "/Volumes/USB/data.json"}),
        ("Edit", {"file_path": "/Volumes/Backup/config.yml"}),
        ("Glob", {"path": "/Volumes/TimeMachine"}),
        ("Grep", {"path": "/Volumes/External"}),
        ("Bash", {"command": "cat /Volumes/MyDrive/secret.txt"}),
        ("Bash", {"command": "ls /Volumes/USB-Stick/"}),
    ])
    def test_external_drive_blocked(self, tool_name, tool_input, monkeypatch):
        """Verify external drive access is blocked by default."""
        monkeypatch.setenv('SANDVAULT_ALLOW_EXTERNAL_DRIVES', '0')
        # Re-import to pick up env change
        import pre_tool_use
        importlib.reload(pre_tool_use)
        assert pre_tool_use.is_external_drive_access(tool_name, tool_input) is True

    @pytest.mark.parametrize("tool_name,tool_input", [
        ("Read", {"file_path": "/Volumes/Macintosh HD/Users/test/file.txt"}),
        ("Write", {"file_path": "/Volumes/Macintosh HD/tmp/data.json"}),
        ("Read", {"file_path": "/Volumes/Recovery/log.txt"}),
        ("Read", {"file_path": "/Users/test/Documents/file.txt"}),
        ("Bash", {"command": "ls /Users/test"}),
        ("Bash", {"command": "cat /tmp/test.txt"}),
    ])
    def test_system_volumes_allowed(self, tool_name, tool_input, monkeypatch):
        """Verify system volumes are not blocked."""
        monkeypatch.setenv('SANDVAULT_ALLOW_EXTERNAL_DRIVES', '0')
        import pre_tool_use
        importlib.reload(pre_tool_use)
        assert pre_tool_use.is_external_drive_access(tool_name, tool_input) is False

    def test_external_drives_allowed_with_env(self, monkeypatch):
        """Verify external drives allowed when env var is set."""
        monkeypatch.setenv('SANDVAULT_ALLOW_EXTERNAL_DRIVES', '1')
        import pre_tool_use
        importlib.reload(pre_tool_use)
        assert pre_tool_use.is_external_drive_access(
            "Read", {"file_path": "/Volumes/ExternalDrive/file.txt"}
        ) is False


class TestEnvFileAccess:
    """Tests for .env file access blocking."""

    @pytest.mark.parametrize("tool_name,tool_input", [
        ("Read", {"file_path": "/project/.env"}),
        ("Read", {"file_path": ".env"}),
        ("Write", {"file_path": "/app/.env"}),
        ("Edit", {"file_path": ".env.local"}),
        ("Edit", {"file_path": ".env.production"}),
        ("Bash", {"command": "cat .env"}),
        ("Bash", {"command": "cp backup.env .env"}),
    ])
    def test_env_files_blocked(self, tool_name, tool_input):
        """Verify .env file access is blocked."""
        assert is_env_file_access(tool_name, tool_input) is True

    @pytest.mark.parametrize("tool_name,tool_input", [
        ("Read", {"file_path": "/project/.env.sample"}),
        ("Read", {"file_path": ".env.sample"}),
        ("Write", {"file_path": ".env.sample"}),
        ("Read", {"file_path": "/project/config.json"}),
        ("Bash", {"command": "cat config.json"}),
        ("Bash", {"command": "ls -la"}),
    ])
    def test_env_sample_allowed(self, tool_name, tool_input):
        """Verify .env.sample and other files are allowed."""
        assert is_env_file_access(tool_name, tool_input) is False
