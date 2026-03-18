#!/usr/bin/env python3
"""
VibeVoice TTS Provider
Streaming text-to-speech using VibeVoice 7B model
"""

import asyncio
import base64
import json
import sys
from typing import AsyncIterator, Optional, Tuple
import httpx
import numpy as np


class VibeVoiceProvider:
    """VibeVoice streaming TTS provider"""

    def __init__(self, config: dict):
        self.base_url = config.get("base_url", "http://localhost:5381")
        self.voice = config.get("voice", "alloy")
        self.streaming = config.get("streaming", True)
        self.cfg_scale = config.get("cfg_scale", 1.3)
        self.inference_steps = config.get("inference_steps", 10)
        self.client = httpx.AsyncClient(timeout=30.0)

    async def synthesize_streaming(
        self,
        text: str,
        voice: Optional[str] = None
    ) -> AsyncIterator[Tuple[np.ndarray, int]]:
        """
        Stream TTS audio chunks as they are generated.

        Args:
            text: Text to synthesize
            voice: Voice preset (alloy, echo, fable, onyx, nova, shimmer)

        Yields:
            Tuple of (audio_chunk, sample_rate)
            audio_chunk is np.ndarray of float32 PCM samples
        """
        url = f"{self.base_url}/v1/vibevoice/stream"
        payload = {
            "text": text,
            "voice": voice or self.voice,
            "cfg_scale": self.cfg_scale,
            "inference_steps": self.inference_steps,
        }

        try:
            async with self.client.stream("POST", url, json=payload) as response:
                response.raise_for_status()

                buffer = ""
                async for chunk in response.aiter_text():
                    buffer += chunk
                    lines = buffer.split("\n")
                    buffer = lines.pop()  # Keep incomplete line

                    for line in lines:
                        if not line.startswith("data: "):
                            continue

                        try:
                            event = json.loads(line[6:])
                        except json.JSONDecodeError:
                            continue

                        if event["type"] == "audio_chunk":
                            # Decode base64 PCM audio
                            audio_bytes = base64.b64decode(event["data"])
                            audio = np.frombuffer(audio_bytes, dtype=np.float32)
                            sample_rate = event.get("sample_rate", 24000)
                            yield audio, sample_rate

                        elif event["type"] == "done":
                            break

                        elif event["type"] == "error":
                            raise RuntimeError(
                                f"VibeVoice streaming error: {event.get('message')}"
                            )

        except httpx.HTTPStatusError as e:
            if e.response.status_code == 503:
                raise RuntimeError("VibeVoice server not ready or model not loaded")
            raise
        except httpx.ConnectError:
            raise RuntimeError(
                f"Could not connect to VibeVoice at {self.base_url}. "
                "Is the server running?"
            )

    async def synthesize(self, text: str, voice: Optional[str] = None) -> bytes:
        """
        Non-streaming synthesis (returns complete WAV file).

        Args:
            text: Text to synthesize
            voice: Voice preset

        Returns:
            WAV audio data as bytes
        """
        url = f"{self.base_url}/v1/audio/speech"
        payload = {
            "model": "vibevoice-7b",
            "input": text,
            "voice": voice or self.voice,
            "response_format": "wav"
        }

        try:
            response = await self.client.post(url, json=payload)
            response.raise_for_status()
            return response.content
        except httpx.ConnectError:
            raise RuntimeError(
                f"Could not connect to VibeVoice at {self.base_url}. "
                "Is the server running?"
            )

    async def close(self):
        """Close HTTP client"""
        await self.client.aclose()


async def main():
    """Test VibeVoice provider"""
    if len(sys.argv) < 2:
        print("Usage: python vibevoice.py <text>")
        sys.exit(1)

    text = " ".join(sys.argv[1:])

    config = {
        "base_url": "http://localhost:5381",
        "voice": "alloy",
        "streaming": True,
    }

    provider = VibeVoiceProvider(config)

    print(f"Synthesizing: {text}")
    print("Streaming audio chunks...")

    try:
        chunk_count = 0
        async for audio_chunk, sample_rate in provider.synthesize_streaming(text):
            chunk_count += 1
            print(f"Chunk {chunk_count}: {len(audio_chunk)} samples at {sample_rate}Hz")

        print(f"✓ Streaming complete. Received {chunk_count} chunks.")
    except Exception as e:
        print(f"✗ Error: {e}", file=sys.stderr)
        sys.exit(1)
    finally:
        await provider.close()


if __name__ == "__main__":
    asyncio.run(main())
