#!/usr/bin/env python3
"""
ElevenLabs TTS Provider
High-quality text-to-speech using ElevenLabs API
"""

import asyncio
import os
import sys
from typing import Optional
import httpx


class ElevenLabsProvider:
    """ElevenLabs TTS provider"""

    def __init__(self, config: dict):
        self.api_key = config.get("api_key") or os.getenv("ELEVENLABS_API_KEY")
        if not self.api_key:
            raise ValueError("ElevenLabs API key required (set in config or ELEVENLABS_API_KEY env var)")

        self.voice_id = config.get("voice_id", "21m00Tcm4TlvDq8ikWAM")  # Default voice
        self.model_id = config.get("model_id", "eleven_monolingual_v1")
        self.stability = config.get("stability", 0.5)
        self.similarity_boost = config.get("similarity_boost", 0.75)

        self.client = httpx.AsyncClient(
            base_url="https://api.elevenlabs.io/v1",
            headers={"xi-api-key": self.api_key},
            timeout=30.0
        )

    async def synthesize(
        self,
        text: str,
        voice_id: Optional[str] = None
    ) -> bytes:
        """
        Synthesize speech from text.

        Args:
            text: Text to synthesize
            voice_id: ElevenLabs voice ID

        Returns:
            MP3 audio data as bytes
        """
        url = f"/text-to-speech/{voice_id or self.voice_id}"

        payload = {
            "text": text,
            "model_id": self.model_id,
            "voice_settings": {
                "stability": self.stability,
                "similarity_boost": self.similarity_boost
            }
        }

        try:
            response = await self.client.post(url, json=payload)
            response.raise_for_status()
            return response.content
        except httpx.HTTPStatusError as e:
            if e.response.status_code == 401:
                raise RuntimeError("ElevenLabs API key is invalid")
            elif e.response.status_code == 429:
                raise RuntimeError("ElevenLabs API quota exceeded")
            raise RuntimeError(f"ElevenLabs API error: {e.response.text}")

    async def close(self):
        """Close HTTP client"""
        await self.client.aclose()


async def main():
    """Test ElevenLabs provider"""
    if len(sys.argv) < 2:
        print("Usage: python elevenlabs.py <text>")
        sys.exit(1)

    text = " ".join(sys.argv[1:])

    config = {
        "api_key": os.getenv("ELEVENLABS_API_KEY"),
        "voice_id": "21m00Tcm4TlvDq8ikWAM"
    }

    provider = ElevenLabsProvider(config)

    print(f"Synthesizing: {text}")

    try:
        audio = await provider.synthesize(text)
        print(f"✓ Generated {len(audio)} bytes of audio")

        # Save to file for testing
        output_file = "/tmp/elevenlabs_tts_test.mp3"
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
