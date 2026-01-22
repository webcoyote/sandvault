#!/usr/bin/env python3
"""
Shared TTS (Text-to-Speech) utilities for Claude hooks.
Provides automatic selection and fallback between different TTS services.
"""

import os
import subprocess
from pathlib import Path
from typing import Optional


def get_tts_script_path() -> Optional[str]:
    """
    Determine which TTS script to use based on available API keys.
    Priority order: ElevenLabs > OpenAI > pyttsx3
    
    Returns:
        Path to the appropriate TTS script, or None if no script is available
    """
    # Get the TTS directory path
    utils_dir = Path(__file__).parent
    tts_dir = utils_dir / "tts"
    
    # Check for ElevenLabs API key (highest priority)
    if os.getenv('ELEVENLABS_API_KEY'):
        elevenlabs_script = tts_dir / "elevenlabs_tts.py"
        if elevenlabs_script.exists():
            return str(elevenlabs_script)
    
    # Check for OpenAI API key (second priority)
    if os.getenv('OPENAI_API_KEY'):
        openai_script = tts_dir / "openai_tts.py"
        if openai_script.exists():
            return str(openai_script)
    
    # Fall back to pyttsx3 (no API key required)
    pyttsx3_script = tts_dir / "pyttsx3_tts.py"
    if pyttsx3_script.exists():
        return str(pyttsx3_script)
    
    return None


def speak_text(
    text: str,
    timeout: int = 10,
    suppress_output: bool = True
) -> bool:
    """
    Speak text using the best available TTS service.
    
    Args:
        text: Text to speak
        timeout: Maximum time to wait for TTS completion (default: 10 seconds)
        suppress_output: Whether to suppress TTS script output (default: True)
    
    Returns:
        True if speech was successful, False otherwise
    """
    try:
        tts_script = get_tts_script_path()
        if not tts_script:
            return False  # No TTS scripts available
        
        # Prepare subprocess arguments
        args = ["uv", "run", tts_script, text]
        kwargs = {"timeout": timeout}
        
        if suppress_output:
            kwargs["capture_output"] = True
        
        # Call the TTS script
        result = subprocess.run(args, **kwargs)
        return result.returncode == 0
        
    except (subprocess.TimeoutExpired, subprocess.SubprocessError, FileNotFoundError):
        return False
    except Exception:
        return False
