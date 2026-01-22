# SandVault Tests

This directory contains the test suite for SandVault.

## Prerequisites

- **Bash tests**: No additional dependencies (uses built-in bash)
- **Python tests**: `pytest` (`pip install pytest`)
- **Linting**: `shellcheck` (`brew install shellcheck`) and `ruff` (`pip install ruff`)

## Running Tests Locally

### Run all tests
```bash
./scripts/test-all.sh
```

### Run bash tests only
```bash
./scripts/test-bash.sh
```

### Run Python tests only
```bash
./scripts/test-python.sh
```

### Run linters
```bash
./scripts/lint-all.sh
```

## Test Structure

```
tests/
├── bash/
│   ├── helpers.sh           # Test assertion functions
│   ├── test_version.sh      # Version output tests
│   ├── test_cli.sh          # CLI argument parsing tests
│   └── test_logging.sh      # Logging function tests
├── python/
│   ├── conftest.py          # pytest fixtures
│   ├── test_pre_tool_use.py # Security hook tests
│   ├── test_git.py          # Git utility tests
│   └── test_logging_utils.py# JSONL logging tests
└── README.md                # This file
```

## What's Tested

### Bash Tests
- `--version` and `--help` flags
- CLI argument parsing
- Logging functions (trace, debug, info, warn, error) with VERBOSE levels

### Python Tests
- **Security hooks** (`pre_tool_use.py`):
  - Dangerous `rm -rf` command detection
  - External drive access blocking
  - `.env` file access blocking
- **Git utilities** (`git.py`):
  - Branch name parsing
  - Git status parsing
  - Error handling for missing git
- **Logging utilities** (`logging.py`):
  - JSONL file creation
  - Log directory structure

## CI/CD

Tests run automatically on:
- Push to `main` branch
- Pull requests to `main` branch

See `.github/workflows/ci.yml` for the CI configuration.
