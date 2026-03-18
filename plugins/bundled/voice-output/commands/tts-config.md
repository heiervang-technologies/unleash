# tts-config - Configure Text-to-Speech Settings

Configure TTS provider, voice, and synthesis settings.

## Usage

```bash
/tts-config                      # Interactive configuration
/tts-config --provider vibevoice # Set provider
/tts-config --voice nova         # Set voice
/tts-config --test              # Test current configuration
/tts-config --show              # Show current settings
```

## Description

The `tts-config` command provides an interactive way to configure all aspects of the text-to-speech system, including:

- TTS provider selection (VibeVoice, OpenAI, ElevenLabs, Custom)
- Voice selection for each provider
- Provider-specific settings (quality, speed, etc.)
- Playback and filtering options
- Test current configuration

## Options

- **`--provider <name>`** - Set TTS provider (vibevoice, openai, elevenlabs, custom)
- **`--voice <name>`** - Set voice/voice preset
- **`--test`** - Test current configuration
- **`--show`** - Display current settings
- **`--reset`** - Reset to default configuration
- **`--interactive`** - Launch interactive configuration wizard (default)

## Interactive Mode

When run without arguments, launches an interactive configuration wizard:

```bash
/tts-config

# Output:
# TTS Configuration Wizard
#
# Current Provider: vibevoice
# Current Voice: alloy
#
# Select Provider:
#   1) VibeVoice (Local, Free, Streaming)
#   2) OpenAI TTS (Cloud, Paid, High Quality)
#   3) ElevenLabs (Cloud, Paid, Ultra Realistic)
#   4) Custom Endpoint
#
# Enter choice [1-4]:
```

## Provider Configuration

### VibeVoice (Local)

```bash
/tts-config --provider vibevoice

# Interactive prompts:
# - Base URL (default: http://localhost:5381)
# - Voice preset (alloy, echo, fable, onyx, nova, shimmer)
# - Streaming enabled (yes/no)
# - CFG scale (1.0-2.0, default: 1.3)
# - Inference steps (5-20, default: 10)
```

**Configuration:**
```json
{
  "provider": "vibevoice",
  "vibevoice": {
    "base_url": "http://localhost:5381",
    "voice": "alloy",
    "streaming": true,
    "cfg_scale": 1.3,
    "inference_steps": 10
  }
}
```

**Settings:**
- **base_url**: VibeVoice server address
- **voice**: Voice preset (alloy, echo, fable, onyx, nova, shimmer)
- **streaming**: Enable real-time streaming (true/false)
- **cfg_scale**: Guidance scale for quality (1.0-2.0, higher = more accurate)
- **inference_steps**: Number of diffusion steps (5-20, higher = better quality)

### OpenAI TTS (Cloud)

```bash
/tts-config --provider openai

# Interactive prompts:
# - API Key
# - Model (tts-1, tts-1-hd)
# - Voice (alloy, echo, fable, onyx, nova, shimmer)
# - Speed (0.25-4.0)
```

**Configuration:**
```json
{
  "provider": "openai",
  "openai": {
    "api_key": "sk-...",
    "model": "tts-1-hd",
    "voice": "alloy",
    "speed": 1.0
  }
}
```

**Settings:**
- **api_key**: OpenAI API key (required)
- **model**: `tts-1` (faster) or `tts-1-hd` (higher quality)
- **voice**: Same voices as VibeVoice
- **speed**: Playback speed (0.25-4.0, default: 1.0)

### ElevenLabs (Cloud)

```bash
/tts-config --provider elevenlabs

# Interactive prompts:
# - API Key
# - Voice ID (from ElevenLabs dashboard)
# - Model ID
# - Stability (0.0-1.0)
# - Similarity Boost (0.0-1.0)
```

**Configuration:**
```json
{
  "provider": "elevenlabs",
  "elevenlabs": {
    "api_key": "...",
    "voice_id": "21m00Tcm4TlvDq8ikWAM",
    "model_id": "eleven_monolingual_v1",
    "stability": 0.5,
    "similarity_boost": 0.75
  }
}
```

**Settings:**
- **api_key**: ElevenLabs API key (required)
- **voice_id**: Voice ID from ElevenLabs (get from dashboard)
- **model_id**: Model to use (eleven_monolingual_v1, eleven_multilingual_v1, etc.)
- **stability**: Voice consistency (0.0-1.0, higher = more consistent)
- **similarity_boost**: Voice similarity (0.0-1.0, higher = closer to original)

### Custom Endpoint

```bash
/tts-config --provider custom

# Interactive prompts:
# - Endpoint URL
# - HTTP Method (GET/POST)
# - Headers (JSON format)
# - Request format
```

**Configuration:**
```json
{
  "provider": "custom",
  "custom": {
    "endpoint": "https://my-tts-server.com/synthesize",
    "method": "POST",
    "headers": {
      "Authorization": "Bearer token",
      "Content-Type": "application/json"
    }
  }
}
```

## Voice Configuration

### Set Voice Directly

```bash
# Set voice for current provider
/tts-config --voice nova

# Test the voice
/tts-config --voice echo --test
```

### Available Voices

**VibeVoice/OpenAI voices:**
- **alloy**: Neutral, balanced voice
- **echo**: Male voice with depth
- **fable**: British female voice
- **onyx**: Deep, authoritative male voice
- **nova**: Young, energetic female voice
- **shimmer**: Soft, gentle female voice

**ElevenLabs voices:**
- Custom voice IDs from your ElevenLabs account
- Professional voice library
- Cloned voices

## Playback Settings

```bash
/tts-config --playback

# Interactive prompts:
# - Auto-play enabled (yes/no)
# - Save to file (yes/no)
# - Output directory
# - Audio player (auto, default, or custom)
```

**Configuration:**
```json
{
  "playback": {
    "auto_play": true,
    "save_to_file": false,
    "output_dir": "~/.claude/tts-output",
    "player": "auto"
  }
}
```

**Settings:**
- **auto_play**: Automatically play audio (true/false)
- **save_to_file**: Save audio files automatically (true/false)
- **output_dir**: Directory for saved audio files
- **player**: Audio player to use (auto, default, or path to custom player)

## Filtering Settings

```bash
/tts-config --filtering

# Interactive prompts:
# - Skip code blocks (yes/no)
# - Skip tool calls (yes/no)
# - Maximum text length
# - Chunk on sentences (yes/no)
```

**Configuration:**
```json
{
  "filtering": {
    "skip_code_blocks": true,
    "skip_tool_calls": true,
    "max_length": 4096,
    "chunk_on_sentences": true
  }
}
```

**Settings:**
- **skip_code_blocks**: Don't speak code blocks (true/false)
- **skip_tool_calls**: Don't speak tool execution output (true/false)
- **max_length**: Maximum characters to synthesize (default: 4096)
- **chunk_on_sentences**: Split long text at sentence boundaries (true/false)

## Testing Configuration

```bash
# Test current configuration
/tts-config --test

# Output:
# Testing TTS configuration...
#
# Provider: vibevoice
# Voice: alloy
# Endpoint: http://localhost:5381
#
# Synthesizing test phrase: "This is a test of the text-to-speech system."
#
# ✓ Connection successful
# ✓ Synthesis successful
# ✓ Audio playback successful
#
# Configuration is working correctly.
```

### Advanced Testing

```bash
# Test specific provider
/tts-config --provider openai --test

# Test specific voice
/tts-config --voice echo --test

# Test with custom text
/tts-config --test "Custom test phrase"
```

## Showing Current Configuration

```bash
/tts-config --show

# Output:
# Current TTS Configuration
#
# Status: Enabled
# Provider: vibevoice
#
# VibeVoice Settings:
#   Base URL: http://localhost:5381
#   Voice: alloy
#   Streaming: true
#   CFG Scale: 1.3
#   Inference Steps: 10
#
# Playback Settings:
#   Auto-play: true
#   Save to file: false
#   Output directory: ~/.claude/tts-output
#
# Filtering Settings:
#   Skip code blocks: true
#   Skip tool calls: true
#   Max length: 4096 characters
```

## Resetting Configuration

```bash
# Reset to defaults
/tts-config --reset

# Output:
# ✓ TTS configuration reset to defaults
#
# Default configuration:
#   Provider: vibevoice
#   Voice: alloy
#   Streaming: enabled
#   Auto-play: enabled
```

## Examples

### Setup VibeVoice

```bash
# Configure VibeVoice with custom settings
/tts-config --provider vibevoice

# Follow prompts:
# Base URL: http://localhost:5381
# Voice: echo
# Streaming: yes
# CFG Scale: 1.5
# Inference Steps: 12

# Test configuration
/tts-config --test
```

### Setup OpenAI TTS

```bash
# Configure OpenAI
/tts-config --provider openai

# Follow prompts:
# API Key: sk-...
# Model: tts-1-hd
# Voice: nova
# Speed: 1.0

# Test with custom phrase
/tts-config --test "Hello, this is OpenAI TTS"
```

### Setup ElevenLabs

```bash
# Configure ElevenLabs
/tts-config --provider elevenlabs

# Follow prompts:
# API Key: ...
# Voice ID: 21m00Tcm4TlvDq8ikWAM
# Model: eleven_monolingual_v1
# Stability: 0.6
# Similarity Boost: 0.8

# Test
/tts-config --test
```

### Configure Filtering

```bash
# Set up filtering for code-heavy responses
/tts-config --filtering

# Skip code blocks: yes
# Skip tool calls: yes
# Max length: 2048
# Chunk on sentences: yes
```

## Troubleshooting

### Configuration Not Persisting

**Check:**
```bash
# View settings file
cat ~/.claude/settings.json | grep -A 20 voice-output

# Verify it's valid JSON
jq . ~/.claude/settings.json
```

**Solutions:**
1. Check file permissions: `ls -la ~/.claude/settings.json`
2. Verify JSON syntax is valid
3. Try resetting: `/tts-config --reset`

### Test Fails with Connection Error

**VibeVoice:**
```bash
# Check server is running
curl http://localhost:5381/health

# Start VibeVoice if needed
vibevoice serve --port 5381
```

**OpenAI/ElevenLabs:**
```bash
# Check API key is valid
/tts-config --show

# Verify network connection
ping api.openai.com
```

### Voice Not Available

**Check:**
```bash
# List available voices
/tts-config --provider vibevoice --show

# For ElevenLabs, check dashboard:
# https://elevenlabs.io/voices
```

### Audio Quality Issues

**VibeVoice:**
```bash
# Increase quality settings
/tts-config --provider vibevoice

# Set:
# CFG Scale: 1.5-2.0 (higher quality)
# Inference Steps: 15-20 (slower but better)
```

**OpenAI:**
```bash
# Use HD model
/tts-config --provider openai

# Set:
# Model: tts-1-hd
```

## Configuration File Location

Settings are stored in:
```
~/.claude/settings.json
```

Under the key:
```json
{
  "plugins": {
    "voice-output": {
      // Configuration here
    }
  }
}
```

## Best Practices

1. **Start with defaults** - Use `/tts-config --reset` to get known-good settings
2. **Test after changes** - Always run `/tts-config --test` after configuration
3. **Use VibeVoice locally** - Free, fast, and private
4. **OpenAI for quality** - HD model for best cloud quality
5. **ElevenLabs for realism** - Ultra-realistic voices when quality matters
6. **Enable filtering** - Skip code blocks for cleaner audio
7. **Set max length** - Prevent very long syntheses

## Related Commands

- `/tts-enable` - Enable automatic TTS
- `/tts-disable` - Disable automatic TTS
- `/tts-status` - Show current status
- `/speak` - Manual TTS trigger

## See Also

- [Voice Output Plugin README](../README.md)
- [Provider Setup Guide](../README.md#provider-setup)
- [VibeVoice Documentation](https://github.com/heiervang-technologies/vibevoice)
