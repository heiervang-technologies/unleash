#!/usr/bin/env python3
"""
OpenAI TTS Provider
Text-to-speech using OpenAI's TTS API
"""

import asyncio
import os
import sys
from typing import Optional
import httpx


class OpenAIProvider:
    """OpenAI TTS provider"""

    def __init__(self, config: dict):
        self.api_key = config.get("api_key") or os.getenv("OPENAI_API_KEY")
        if not self.api_key:
            raise ValueError("OpenAI API key required (set in config or OPENAI_API_KEY env var)")

        self.model = config.get("model", "tts-1")
        self.voice = config.get("voice", "alloy")
        self.speed = config.get("speed", 1.0)

        self.client = httpx.AsyncClient(
            base_url="https://api.openai.com/v1",
            headers={"Authorization": f"Bearer {self.api_key}"},
            timeout=30.0
        )

    async def synthesize(self, text: str, voice: Optional[str] = None) -> bytes:
        """
        Synthesize speech from text.

        Args:
            text: Text to synthesize
            voice: Voice preset (alloy, echo, fable, onyx, nova, shimmer)

        Returns:
            MP3 audio data as bytes
        """
        payload = {
            "model": self.model,
            "input": text,
            "voice": voice or self.voice,
            "speed": self.speed,
            "response_format": "mp3"
        }

        try:
            response = await self.client.post("/audio/speech", json=payload)
            response.raise_for_status()
            return response.content
        except httpx.HTTPStatusError as e:
            if e.response.status_code == 401:
                raise RuntimeError("OpenAI API key is invalid")
            elif e.response.status_code == 429:
                raise RuntimeError("OpenAI API rate limit exceeded")
            raise RuntimeError(f"OpenAI API error: {e.response.text}")

    async def close(self):
        """Close HTTP client"""
        await self.client.aclose()


async def main():
    """Test OpenAI provider"""
    if len(sys.argv) < 2:
        print("Usage: python openai_tts.py <text>")
        sys.exit(1)

    text = " ".join(sys.argv[1:])

    config = {
        "api_key": os.getenv("OPENAI_API_KEY"),
        "model": "tts-1",
        "voice": "alloy"
    }

    provider = OpenAIProvider(config)

    print(f"Synthesizing: {text}")

    try:
        audio = await provider.synthesize(text)
        print(f"✓ Generated {len(audio)} bytes of audio")

        # Save to file for testing
        output_file = "/tmp/openai_tts_test.mp3"
        with open(output_file, "wb") as f:
            f.write(audio)
        print(f"✓ Saved to {output_file}")
    except Exception as e:
        print(f"✗ Error: {e}", file=sys.stderr)
        sys.exit(1)
    finally:
        await provider.close()


if __name__ == "__main__":
    asyncio.run(main())
