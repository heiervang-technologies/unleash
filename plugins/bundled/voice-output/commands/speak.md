# speak - Manual Text-to-Speech Trigger

Manually trigger text-to-speech synthesis for specific text or the last Claude response.

## Usage

```bash
/speak <text>          # Speak specific text
/speak                 # Speak the last Claude response
/speak --voice echo    # Use a specific voice
/speak --save output.wav "Hello world"  # Save to file
```

## Description

The `speak` command allows you to manually trigger TTS synthesis without enabling automatic TTS for all responses. This is useful for:

- Testing TTS configuration
- Replaying specific responses
- Generating audio files from text
- Trying different voices

## Options

- **`<text>`** - Text to synthesize. If omitted, uses the last Claude response.
- **`--voice <name>`** - Override the configured voice for this synthesis
- **`--save <file>`** - Save audio to file instead of playing
- **`--provider <name>`** - Override the configured provider (vibevoice, openai, elevenlabs)

## Examples

### Basic Usage

```bash
# Speak the last response
/speak

# Speak specific text
/speak "Hello, this is a test of the text-to-speech system."

# Use a different voice
/speak --voice nova "This is the nova voice."
```

### Voice Options

For VibeVoice (default provider):
```bash
/speak --voice alloy "Default neutral voice"
/speak --voice echo "Male voice with depth"
/speak --voice fable "British female voice"
/speak --voice onyx "Deep male voice"
/speak --voice nova "Young female voice"
/speak --voice shimmer "Soft female voice"
```

### Save to File

```bash
# Save to WAV file
/speak --save announcement.wav "Important announcement for all team members."

# Save with custom voice
/speak --voice onyx --save deep_voice.wav "This is a deep voice recording."
```

### Provider Override

```bash
# Use OpenAI TTS instead of VibeVoice
/speak --provider openai "Test with OpenAI TTS"

# Use ElevenLabs
/speak --provider elevenlabs "Test with ElevenLabs"
```

## Configuration

The command respects your TTS configuration in `.claude/settings.json`:

```json
{
  "plugins": {
    "voice-output": {
      "provider": "vibevoice",
      "vibevoice": {
        "base_url": "http://localhost:5381",
        "voice": "alloy"
      }
    }
  }
}
```

## Requirements

### VibeVoice (default)
- VibeVoice server running at `http://localhost:5381`
- No API key required
- Supports streaming for real-time playback

### OpenAI TTS
- Valid OpenAI API key in configuration
- Network connection required
- Non-streaming (plays after full generation)

### ElevenLabs
- Valid ElevenLabs API key in configuration
- Voice ID configured
- Network connection required

## Playback

By default, audio is played through your system's default audio output device. You can configure playback behavior:

```json
{
  "plugins": {
    "voice-output": {
      "playback": {
        "auto_play": true,
        "player": "auto"
      }
    }
  }
}
```

## Text Processing

Text is automatically filtered before synthesis:
- Code blocks removed (if `skip_code_blocks: true`)
- Markdown formatting stripped
- URLs converted to readable text
- Limited to `max_length` characters (default: 4096)

## Troubleshooting

### No audio plays

**Check:**
1. Is the TTS provider running? (VibeVoice: `http://localhost:5381`)
2. Is your audio output device working?
3. Is `sounddevice` installed? (`pip install sounddevice`)

**Solution:**
```bash
# Test VibeVoice connection
curl http://localhost:5381/health

# Test audio playback
python -c "import sounddevice; sounddevice.play([0.1] * 4800, 24000); sounddevice.wait()"

# Use --save to verify synthesis works
/speak --save test.wav "Test audio"
```

### "Provider not initialized" error

**Check:**
- Is the provider configured in settings?
- For OpenAI/ElevenLabs: Is the API key valid?
- For VibeVoice: Is the server running?

**Solution:**
```bash
# Check configuration
cat ~/.claude/settings.json | grep -A 10 voice-output

# Verify provider
/tts-status
```

### Audio quality issues

**VibeVoice:**
```json
{
  "vibevoice": {
    "cfg_scale": 1.5,
    "inference_steps": 15
  }
}
```

**OpenAI:**
```json
{
  "openai": {
    "model": "tts-1-hd",
    "speed": 1.0
  }
}
```

### Voice not changing

Make sure to specify the voice option:
```bash
# Wrong: Uses default voice
/speak "Test"

# Correct: Uses specified voice
/speak --voice echo "Test"
```

Or change the default in settings:
```json
{
  "vibevoice": {
    "voice": "echo"
  }
}
```

## Related Commands

- `/tts-enable` - Enable automatic TTS for all responses
- `/tts-disable` - Disable automatic TTS
- `/tts-config` - Configure TTS settings
- `/tts-status` - Show current TTS configuration

## Technical Details

### Audio Format

**VibeVoice streaming:**
- Format: Float32 PCM
- Sample rate: 24kHz
- Channels: Mono
- Delivered via Server-Sent Events (SSE)

**Saved files:**
- Format: WAV
- Sample width: 16-bit
- Sample rate: 24kHz (VibeVoice) or provider-specific
- Channels: Mono

### Processing Pipeline

```
User Input
    ↓
Text Filtering
    ↓
Provider Selection
    ↓
TTS Synthesis
    ↓
Audio Streaming/Playback
    ↓
Optional File Save
```

## Examples by Use Case

### Testing Configuration

```bash
# Quick test
/speak "Test one two three"

# Test all voices
for voice in alloy echo fable onyx nova shimmer; do
  /speak --voice $voice "This is the $voice voice"
done
```

### Creating Audio Files

```bash
# Generate announcement
/speak --save announcement.wav "Please join the all-hands meeting at 2 PM."

# Generate with specific voice
/speak --voice onyx --save narrator.wav "Chapter one: The beginning."
```

### Replaying Responses

```bash
# After Claude gives a long explanation
/speak

# Replay with different voice
/speak --voice fable
```

## Performance Notes

- **VibeVoice streaming**: Latency ~500ms, real-time playback
- **OpenAI TTS**: Latency ~2-3 seconds, full file download
- **ElevenLabs**: Latency ~1-2 seconds, streaming available
- **File save**: Adds minimal overhead (< 100ms)

## Privacy and Security

- **VibeVoice**: Runs locally, no data sent to external servers
- **OpenAI/ElevenLabs**: Text sent to provider's servers
- **Saved files**: Stored locally in specified location
- **No text logging**: Plugin does not log synthesized text

## See Also

- [Voice Output Plugin README](../README.md)
- [VibeVoice Documentation](https://github.com/heiervang-technologies/vibevoice)
- [TTS Configuration Guide](../README.md#configuration)
