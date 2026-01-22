"""Tests for logging.py utility module."""
import importlib
import json
from pathlib import Path
from unittest.mock import patch


class TestLogToJsonl:
    """Tests for log_to_jsonl function."""

    def test_creates_log_file(self, tmp_path):
        """Test that log file is created."""
        log_dir = tmp_path / "logs"
        log_dir.mkdir(parents=True, exist_ok=True)

        with patch('utils.logging.ensure_log_directory') as mock_ensure:
            mock_ensure.return_value = log_dir

            from utils.logging import log_to_jsonl
            test_data = {"event": "test", "value": 123}
            log_to_jsonl(test_data, "test.jsonl")

            log_file = log_dir / "test.jsonl"
            assert log_file.exists()

            with open(log_file) as f:
                content = f.read()
                parsed = json.loads(content.strip())
                assert parsed == test_data

    def test_appends_to_existing_file(self, tmp_path):
        """Test that data is appended to existing log file."""
        log_dir = tmp_path / "logs"
        log_dir.mkdir(parents=True, exist_ok=True)

        with patch('utils.logging.ensure_log_directory') as mock_ensure:
            mock_ensure.return_value = log_dir

            from utils.logging import log_to_jsonl

            # Write first entry
            log_to_jsonl({"entry": 1}, "test.jsonl")
            # Write second entry
            log_to_jsonl({"entry": 2}, "test.jsonl")

            log_file = log_dir / "test.jsonl"
            with open(log_file) as f:
                lines = f.readlines()
                assert len(lines) == 2
                assert json.loads(lines[0])["entry"] == 1
                assert json.loads(lines[1])["entry"] == 2


class TestEnsureLogDirectory:
    """Tests for ensure_log_directory function."""

    def test_creates_directory_structure(self, tmp_path, monkeypatch):
        """Test that directory structure is created."""
        # Mock Path.home() to use tmp_path
        monkeypatch.setattr(Path, 'home', lambda: tmp_path)
        monkeypatch.chdir(tmp_path)

        # Create a fake git repo directory
        (tmp_path / ".git").mkdir()

        with patch('utils.git.get_git_branch') as mock_branch:
            mock_branch.return_value = "main"

            # Need to reload the module to pick up the mocked home
            import utils.logging
            importlib.reload(utils.logging)

            result = utils.logging.ensure_log_directory()
            assert result.exists()
            assert "main" in str(result)
