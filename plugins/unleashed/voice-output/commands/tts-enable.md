# tts-enable - Enable Automatic Text-to-Speech

Enable automatic text-to-speech for all Claude responses.

## Usage

```bash
/tts-enable           # Enable auto-TTS with current settings
/tts-enable --voice echo    # Enable with specific voice
/tts-enable --provider openai  # Enable with specific provider
```

## Description

The `tts-enable` command activates automatic text-to-speech synthesis for all Claude responses. When enabled, every response from Claude will be automatically spoken using the configured TTS provider.

This is useful for:
- Hands-free interaction with Claude
- Accessibility for visually impaired users
- Multitasking while receiving responses
- Auditory learning preferences

## Options

- **`--voice <name>`** - Set the voice to use (persists in settings)
- **`--provider <name>`** - Set the provider to use (vibevoice, openai, elevenlabs)
- **`--no-save`** - Disable file saving (playback only)

## Behavior

When automatic TTS is enabled:

1. **After each Claude response**, the PostToolUse hook triggers
2. **Response text is filtered** (code blocks, markdown removed)
3. **TTS synthesis begins** using configured provider
4. **Audio plays automatically** through default output device
5. **Optionally saves** to file if `save_to_file: true`

## Examples

### Basic Enable

```bash
# Enable with default settings
/tts-enable

# Output:
# ✓ Automatic TTS enabled
# Provider: vibevoice
# Voice: alloy
# All Claude responses will now be spoken.
```

### Enable with Voice Selection

```bash
# Enable with specific voice
/tts-enable --voice nova

# Enable with deep male voice
/tts-enable --voice onyx
```

### Enable with Provider

```bash
# Use OpenAI TTS
/tts-enable --provider openai

# Use ElevenLabs
/tts-enable --provider elevenlabs --voice <voice-id>
```

## Configuration

The command updates `.claude/settings.json`:

```json
{
  "plugins": {
    "voice-output": {
      "enabled": true,
      "provider": "vibevoice",
      "vibevoice": {
        "voice": "alloy"
      }
    }
  }
}
```

## What Gets Spoken

By default, automatic TTS will speak:
- All text responses from Claude
- Error messages
- Status updates

It will **not** speak (configurable):
- Code blocks (if `skip_code_blocks: true`)
- Tool execution output (if `skip_tool_calls: true`)
- Responses longer than `max_length` (truncated with "...")

## Filtering Configuration

Control what gets spoken:

```json
{
  "plugins": {
    "voice-output": {
      "filtering": {
        "skip_code_blocks": true,
        "skip_tool_calls": true,
        "max_length": 4096,
        "chunk_on_sentences": true
      }
    }
  }
}
```

## Playback Configuration

Control how audio is played:

```json
{
  "plugins": {
    "voice-output": {
      "playback": {
        "auto_play": true,
        "save_to_file": false,
        "output_dir": "~/.claude/tts-output",
        "player": "auto"
      }
    }
  }
}
```

## Provider-Specific Setup

### VibeVoice (Local, Free)

**Requirements:**
1. VibeVoice server running at `localhost:5381`
2. VibeVoice 7B model loaded

**Enable:**
```bash
/tts-enable --provider vibevoice
```

**Features:**
- Real-time streaming
- Low latency (~500ms)
- No API costs
- Privacy-friendly (local)

### OpenAI TTS (Cloud, Paid)

**Requirements:**
1. Valid OpenAI API key
2. Network connection

**Setup:**
```json
{
  "openai": {
    "api_key": "sk-...",
    "model": "tts-1-hd",
    "voice": "alloy",
    "speed": 1.0
  }
}
```

**Enable:**
```bash
/tts-enable --provider openai --voice nova
```

**Features:**
- High quality (HD model)
- Multiple voices
- No local setup needed
- Pay per character

### ElevenLabs (Cloud, Paid)

**Requirements:**
1. Valid ElevenLabs API key
2. Voice ID from ElevenLabs dashboard

**Setup:**
```json
{
  "elevenlabs": {
    "api_key": "...",
    "voice_id": "...",
    "model_id": "eleven_monolingual_v1",
    "stability": 0.5,
    "similarity_boost": 0.75
  }
}
```

**Enable:**
```bash
/tts-enable --provider elevenlabs
```

**Features:**
- Ultra-realistic voices
- Voice cloning
- Multilingual support
- Premium pricing

## Disabling

To turn off automatic TTS:

```bash
/tts-disable
```

Or manually:
```bash
# Quick toggle
/tts-enable    # Enable
/tts-disable   # Disable
/tts-enable    # Enable again
```

## Performance Impact

### VibeVoice Streaming
- Minimal latency impact (~500ms)
- Real-time streaming means audio starts quickly
- Low CPU/memory overhead
- No network latency

### OpenAI/ElevenLabs
- Higher latency (2-3 seconds for full response)
- Network dependent
- Audio plays after full generation
- Negligible local resource usage

## Troubleshooting

### TTS Not Working After Enable

**Check:**
```bash
# Verify it's enabled
/tts-status

# Test manually
/speak "Test"
```

**Solutions:**
1. Check provider is running (VibeVoice: `curl http://localhost:5381/health`)
2. Verify API keys (OpenAI/ElevenLabs)
3. Check audio output device is working
4. Review logs: `tail -f ~/.claude/logs/voice-output.log`

### Audio Plays But Quality is Poor

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
    "model": "tts-1-hd"
  }
}
```

### Responses Too Long

```json
{
  "filtering": {
    "max_length": 2048
  }
}
```

### Code Blocks Being Spoken

```json
{
  "filtering": {
    "skip_code_blocks": true
  }
}
```

### Audio Cutting Off

**VibeVoice:** Check server isn't overloaded
```bash
curl http://localhost:5381/health
```

**OpenAI/ElevenLabs:** Check network connection
```bash
ping api.openai.com
```

## Use Cases

### Accessibility

```bash
# Enable for hands-free interaction
/tts-enable --voice nova

# Higher quality for better clarity
/tts-enable --provider openai --voice alloy
```

### Multitasking

```bash
# Listen to responses while coding
/tts-enable --voice echo

# Save responses for later review
/tts-enable --save
```

### Learning

```bash
# Auditory learning preference
/tts-enable --voice fable

# Slower playback for comprehension
# (Configure in settings: "speed": 0.9)
```

## Best Practices

1. **Start with VibeVoice** for testing (free, local)
2. **Use specific voices** for different contexts
3. **Enable filtering** to skip code and tools
4. **Set max_length** to avoid very long syntheses
5. **Monitor costs** if using paid providers

## Privacy Considerations

- **VibeVoice**: All processing local, no data leaves your machine
- **OpenAI/ElevenLabs**: Text sent to provider's servers
- **File saving**: Audio stored locally in `output_dir`
- **No telemetry**: Plugin doesn't send usage data anywhere

## Related Commands

- `/tts-disable` - Disable automatic TTS
- `/speak` - Manual TTS trigger
- `/tts-config` - Configure TTS settings
- `/tts-status` - Show current configuration

## Technical Details

### Hook Integration

When enabled, the PostToolUse hook:
1. Receives Claude's response text
2. Filters text based on configuration
3. Calls TTS provider API
4. Streams/plays audio
5. Optionally saves to file

### State Persistence

Settings are persisted in `.claude/settings.json`:
```json
{
  "plugins": {
    "voice-output": {
      "enabled": true
    }
  }
}
```

Remains enabled across:
- Claude Code restarts
- System reboots
- Configuration reloads

### Resource Usage

- **VibeVoice**: ~2GB RAM (model), ~10% CPU during synthesis
- **OpenAI/ElevenLabs**: Minimal local resources, network bandwidth
- **Audio playback**: ~5MB RAM, negligible CPU

## See Also

- [Voice Output Plugin README](../README.md)
- [TTS Configuration Guide](../README.md#configuration)
- [Provider Setup](../README.md#provider-setup)
