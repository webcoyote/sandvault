#!/usr/bin/env python3
"""
Shared LLM (Large Language Model) utilities for Claude hooks.
Provides automatic selection and fallback between different LLM services.
"""

import os
import subprocess
from pathlib import Path
from typing import Optional


def get_llm_completion(
    prompt_type: str = "completion",
    timeout: int = 10
) -> Optional[str]:
    """
    Generate text using available LLM services.
    Priority order: OpenAI > Anthropic > Fallback
    
    Args:
        prompt_type: Type of prompt to generate (e.g., "completion")
        timeout: Maximum time to wait for LLM response
    
    Returns:
        Generated text or None if no LLM is available
    """
    # Get the LLM directory path
    utils_dir = Path(__file__).parent
    llm_dir = utils_dir / "llm"
    
    # Try OpenAI first (highest priority)
    if os.getenv('OPENAI_API_KEY'):
        oai_script = llm_dir / "oai.py"
        if oai_script.exists():
            try:
                result = subprocess.run(
                    ["uv", "run", str(oai_script), f"--{prompt_type}"],
                    capture_output=True,
                    text=True,
                    timeout=timeout
                )
                if result.returncode == 0 and result.stdout.strip():
                    return result.stdout.strip()
            except (subprocess.TimeoutExpired, subprocess.SubprocessError):
                pass
    
    # Try Anthropic second
    if os.getenv('ANTHROPIC_API_KEY'):
        anth_script = llm_dir / "anth.py"
        if anth_script.exists():
            try:
                result = subprocess.run(
                    ["uv", "run", str(anth_script), f"--{prompt_type}"],
                    capture_output=True,
                    text=True,
                    timeout=timeout
                )
                if result.returncode == 0 and result.stdout.strip():
                    return result.stdout.strip()
            except (subprocess.TimeoutExpired, subprocess.SubprocessError):
                pass
    
    # Try fallback script last (always available)
    fallback_script = llm_dir / "fallback.py"
    if fallback_script.exists():
        try:
            result = subprocess.run(
                ["uv", "run", str(fallback_script), f"--{prompt_type}"],
                capture_output=True,
                text=True,
                timeout=timeout
            )
            if result.returncode == 0 and result.stdout.strip():
                return result.stdout.strip()
        except (subprocess.TimeoutExpired, subprocess.SubprocessError):
            pass

    return None
