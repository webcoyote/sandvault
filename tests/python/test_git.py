"""Tests for git.py utility module."""
import subprocess
from unittest.mock import MagicMock, patch

from utils.git import get_git_branch, get_git_status


class TestGetGitBranch:
    """Tests for get_git_branch function."""

    def test_returns_branch_name(self):
        """Test that branch name is returned in a git repo."""
        with patch('subprocess.run') as mock_run:
            mock_run.return_value = MagicMock(
                returncode=0,
                stdout="main\n"
            )
            result = get_git_branch()
            assert result == "main"
            mock_run.assert_called_once()

    def test_returns_feature_branch(self):
        """Test feature branch name parsing."""
        with patch('subprocess.run') as mock_run:
            mock_run.return_value = MagicMock(
                returncode=0,
                stdout="feature/add-tests\n"
            )
            result = get_git_branch()
            assert result == "feature/add-tests"

    def test_returns_none_not_git_repo(self):
        """Test returns None when not in git repo."""
        with patch('subprocess.run') as mock_run:
            mock_run.return_value = MagicMock(
                returncode=128,
                stdout=""
            )
            result = get_git_branch()
            assert result is None

    def test_handles_timeout(self):
        """Test handles subprocess timeout gracefully."""
        with patch('subprocess.run') as mock_run:
            mock_run.side_effect = subprocess.TimeoutExpired(cmd="git", timeout=2)
            result = get_git_branch()
            assert result is None

    def test_handles_file_not_found(self):
        """Test handles git not installed."""
        with patch('subprocess.run') as mock_run:
            mock_run.side_effect = FileNotFoundError()
            result = get_git_branch()
            assert result is None


class TestGetGitStatus:
    """Tests for get_git_status function."""

    def test_returns_branch_and_count(self):
        """Test returns tuple of branch and change count."""
        with patch('utils.git.get_git_branch') as mock_branch:
            mock_branch.return_value = "main"
            with patch('subprocess.run') as mock_run:
                mock_run.return_value = MagicMock(
                    returncode=0,
                    stdout=" M file1.txt\n M file2.txt\n"
                )
                branch, count = get_git_status()
                assert branch == "main"
                assert count == 2

    def test_clean_repo(self):
        """Test clean repository returns zero changes."""
        with patch('utils.git.get_git_branch') as mock_branch:
            mock_branch.return_value = "develop"
            with patch('subprocess.run') as mock_run:
                mock_run.return_value = MagicMock(returncode=0, stdout="")
                branch, count = get_git_status()
                assert branch == "develop"
                assert count == 0

    def test_not_git_repo(self):
        """Test returns None values when not in git repo."""
        with patch('utils.git.get_git_branch') as mock_branch:
            mock_branch.return_value = None
            with patch('subprocess.run') as mock_run:
                mock_run.return_value = MagicMock(returncode=128, stdout="")
                branch, count = get_git_status()
                assert branch is None
