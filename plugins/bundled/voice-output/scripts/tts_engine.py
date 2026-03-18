#!/usr/bin/env python3
"""
TTS Engine
Main orchestrator for multi-provider text-to-speech
"""

import asyncio
import json
import os
import re
import sys
from pathlib import Path
from typing import Optional

# Add providers directory to path
sys.path.insert(0, str(Path(__file__).parent.parent / "providers"))

from vibevoice import VibeVoiceProvider
from openai_tts import OpenAIProvider
from elevenlabs import ElevenLabsProvider


class TTSEngine:
    """Multi-provider TTS engine"""

    def __init__(self, config_path: Optional[str] = None):
        """Initialize TTS engine with configuration"""
        self.config = self._load_config(config_path)
        self.provider = None
        self._initialize_provider()

    def _load_config(self, config_path: Optional[str] = None) -> dict:
        """Load configuration from file or use defaults"""
        if not config_path:
            config_path = os.path.expanduser("~/.claude/settings.json")

        if os.path.exists(config_path):
            with open(config_path) as f:
                settings = json.load(f)
                return settings.get("plugins", {}).get("voice-output", {})

        # Default configuration
        return {
            "enabled": False,
            "provider": "vibevoice",
            "vibevoice": {
                "base_url": "http://localhost:5381",
                "voice": "alloy",
                "streaming": True
            }
        }

    def _initialize_provider(self):
        """Initialize the selected TTS provider"""
        provider_name = self.config.get("provider", "vibevoice")

        try:
            if provider_name == "vibevoice":
                self.provider = VibeVoiceProvider(
                    self.config.get("vibevoice", {})
                )
            elif provider_name == "openai":
                self.provider = OpenAIProvider(
                    self.config.get("openai", {})
                )
            elif provider_name == "elevenlabs":
                self.provider = ElevenLabsProvider(
                    self.config.get("elevenlabs", {})
                )
            else:
                raise ValueError(f"Unknown provider: {provider_name}")
        except Exception as e:
            print(f"Error initializing {provider_name} provider: {e}", file=sys.stderr)
            self.provider = None

    def _filter_text(self, text: str) -> str:
        """Filter text before synthesis"""
        filtering = self.config.get("filtering", {})

        # Skip code blocks if enabled
        if filtering.get("skip_code_blocks", True):
            # Remove code blocks (```...```)
            text = re.sub(r'```[\s\S]*?```', '', text)
            # Remove inline code (`...`)
            text = re.sub(r'`[^`]+`', '', text)

        # Remove markdown formatting
        text = re.sub(r'\*\*([^*]+)\*\*', r'\1', text)  # Bold
        text = re.sub(r'\*([^*]+)\*', r'\1', text)  # Italic
        text = re.sub(r'__([^_]+)__', r'\1', text)  # Bold alt
        text = re.sub(r'_([^_]+)_', r'\1', text)  # Italic alt

        # Remove links but keep text
        text = re.sub(r'\[([^\]]+)\]\([^\)]+\)', r'\1', text)

        # Remove excessive whitespace
        text = re.sub(r'\n\s*\n', '\n\n', text)
        text = text.strip()

        # Enforce max length
        max_length = filtering.get("max_length", 4096)
        if len(text) > max_length:
            text = text[:max_length] + "..."

        return text

    async def synthesize(
        self,
        text: str,
        voice: Optional[str] = None,
        output_file: Optional[str] = None
    ) -> Optional[bytes]:
        """
        Synthesize text to speech.

        Args:
            text: Text to synthesize
            voice: Optional voice override
            output_file: Optional file to save audio

        Returns:
            Audio bytes if not streaming, None if streaming
        """
        if not self.provider:
            raise RuntimeError("TTS provider not initialized")

        # Filter text
        filtered_text = self._filter_text(text)
        if not filtered_text:
            print("No text to synthesize after filtering", file=sys.stderr)
            return None

        # Check if provider supports streaming
        provider_name = self.config.get("provider")
        is_streaming = (
            provider_name == "vibevoice" and
            self.config.get("vibevoice", {}).get("streaming", True)
        )

        if is_streaming:
            # Streaming synthesis
            await self._synthesize_streaming(filtered_text, voice, output_file)
            return None
        else:
            # Non-streaming synthesis
            audio = await self.provider.synthesize(filtered_text, voice)

            if output_file:
                with open(output_file, "wb") as f:
                    f.write(audio)
                print(f"Saved audio to {output_file}")

            return audio

    async def _synthesize_streaming(
        self,
        text: str,
        voice: Optional[str] = None,
        output_file: Optional[str] = None
    ):
        """Synthesize with streaming (VibeVoice)"""
        import numpy as np

        # Import audio playback
        try:
            import sounddevice as sd
        except ImportError:
            print("sounddevice not installed. Install with: pip install sounddevice")
            sd = None

        chunks = []
        chunk_count = 0

        async for audio_chunk, sample_rate in self.provider.synthesize_streaming(text, voice):
            chunk_count += 1
            chunks.append(audio_chunk)

            # Play chunk if auto_play enabled and sounddevice available
            playback = self.config.get("playback", {})
            if playback.get("auto_play", True) and sd:
                try:
                    sd.play(audio_chunk, sample_rate)
                    sd.wait()
                except Exception as e:
                    print(f"Playback error: {e}", file=sys.stderr)

        print(f"Received {chunk_count} audio chunks")

        # Save to file if requested
        if output_file and chunks:
            # Concatenate chunks
            full_audio = np.concatenate(chunks)

            # Save as WAV
            import wave
            import struct

            with wave.open(output_file, 'wb') as wav_file:
                wav_file.setnchannels(1)  # Mono
                wav_file.setsampwidth(2)  # 16-bit
                wav_file.setframerate(sample_rate)

                # Convert float32 to int16
                int16_audio = (full_audio * 32767).astype(np.int16)
                wav_file.writeframes(int16_audio.tobytes())

            print(f"Saved audio to {output_file}")

    async def close(self):
        """Clean up resources"""
        if self.provider:
            await self.provider.close()


async def main():
    """Test TTS engine"""
    if len(sys.argv) < 2:
        print("Usage: python tts_engine.py <text> [output_file]")
        sys.exit(1)

    text = sys.argv[1]
    output_file = sys.argv[2] if len(sys.argv) > 2 else None

    engine = TTSEngine()

    print(f"Provider: {engine.config.get('provider')}")
    print(f"Synthesizing: {text[:100]}...")

    try:
        await engine.synthesize(text, output_file=output_file)
        print("✓ Synthesis complete")
    except Exception as e:
        print(f"✗ Error: {e}", file=sys.stderr)
        import traceback
        traceback.print_exc()
        sys.exit(1)
    finally:
        await engine.close()


if __name__ == "__main__":
    asyncio.run(main())
