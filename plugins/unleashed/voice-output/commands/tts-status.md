# tts-status - Show TTS Status and Configuration

Display current TTS configuration, provider status, and test connectivity.

## Usage

```bash
/tts-status              # Show basic status
/tts-status --verbose    # Show detailed configuration
/tts-status --test       # Test connectivity to provider
/tts-status --voices     # Show available voices
```

## Description

The `tts-status` command provides comprehensive information about your TTS configuration, including:

- Current enable/disable status
- Active provider and settings
- Provider connectivity
- Available voices
- Configuration file location
- Last synthesis status

## Options

- **`--verbose`** - Show detailed configuration including all settings
- **`--test`** - Test connectivity to the TTS provider
- **`--voices`** - List available voices for current provider
- **`--health`** - Check provider health/availability

## Basic Status

```bash
/tts-status

# Output:
# TTS Status
#
# Status: Enabled
# Provider: vibevoice
# Voice: alloy
# Streaming: Yes
#
# Last synthesis: 2 minutes ago
# Audio output: Default device
```

## Verbose Status

```bash
/tts-status --verbose

# Output:
# TTS Status (Detailed)
#
# General:
#   Status: Enabled
#   Provider: vibevoice
#   Configuration: ~/.claude/settings.json
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
#   Player: auto (using sounddevice)
#
# Filtering Settings:
#   Skip code blocks: true
#   Skip tool calls: true
#   Max length: 4096 characters
#   Chunk on sentences: true
#
# Statistics:
#   Total syntheses: 47
#   Total audio time: 12m 34s
#   Average response time: 1.2s
```

## Connectivity Test

```bash
/tts-status --test

# Output:
# Testing TTS Provider Connection
#
# Provider: vibevoice
# Endpoint: http://localhost:5381
#
# ✓ Server reachable
# ✓ Health check passed
# ✓ Model loaded: vibevoice-7b
# ✓ Streaming available
#
# Response time: 145ms
# Server version: 1.0.0
#
# Status: Healthy
```

## Voice Listing

```bash
/tts-status --voices

# Output (VibeVoice/OpenAI):
# Available Voices
#
# Provider: vibevoice
#
#   alloy     - Neutral, balanced voice (default)
#   echo      - Male voice with depth
#   fable     - British female voice
#   onyx      - Deep, authoritative male voice
#   nova      - Young, energetic female voice
#   shimmer   - Soft, gentle female voice
#
# Current voice: alloy
#
# To change voice: /tts-config --voice <name>
```

```bash
# ElevenLabs voices:
/tts-status --voices

# Output:
# Available Voices
#
# Provider: elevenlabs
# Account: user@example.com
#
# Professional Voices:
#   21m00Tcm4TlvDq8ikWAM - Rachel (Calm, Narrator)
#   AZnzlk1XvdvUeBnXmlld - Domi (Strong, Ads)
#   EXAVITQu4vr4xnSDxMaL - Bella (Soft, Audiobook)
#   ...
#
# Your Cloned Voices:
#   xyz123... - My Voice Clone
#   abc456... - Custom Character
#
# Current voice: 21m00Tcm4TlvDq8ikWAM
```

## Health Check

```bash
/tts-status --health

# Output (Healthy):
# Provider Health Check
#
# Provider: vibevoice
# Endpoint: http://localhost:5381
#
# ✓ Server responding
# ✓ Model loaded
# ✓ GPU available (CUDA 12.1)
# ✓ Sufficient memory (8.2GB free)
# ✓ Streaming endpoint functional
#
# Status: Healthy
# Uptime: 2h 34m
# Processed requests: 147
```

```bash
# Output (Unhealthy):
# Provider Health Check
#
# Provider: vibevoice
# Endpoint: http://localhost:5381
#
# ✗ Connection failed
#
# Error: Could not connect to VibeVoice server
#
# Troubleshooting:
#   1. Check if server is running: curl http://localhost:5381/health
#   2. Start server: vibevoice serve --port 5381
#   3. Verify firewall settings
#
# Status: Unavailable
```

## Status by Provider

### VibeVoice Status

```bash
/tts-status --verbose

# Shows:
# - Server URL and connectivity
# - Model loaded status
# - GPU/CPU mode
# - Memory usage
# - Streaming capability
# - Voice presets available
```

### OpenAI Status

```bash
/tts-status --verbose

# Shows:
# - API key status (valid/invalid)
# - Selected model (tts-1 or tts-1-hd)
# - Current voice
# - Speed setting
# - Account usage (if available)
```

### ElevenLabs Status

```bash
/tts-status --verbose

# Shows:
# - API key status
# - Account email
# - Current voice ID and name
# - Model selection
# - Character count remaining
# - Voice library access
```

## Interpreting Status

### Enabled vs Disabled

**Enabled:**
```
Status: Enabled
```
- Automatic TTS active for all responses
- PostToolUse hook synthesizes responses
- Audio plays automatically (if auto_play: true)

**Disabled:**
```
Status: Disabled
```
- Automatic TTS inactive
- Manual `/speak` still works
- Configuration preserved

### Provider Status

**Connected:**
```
✓ Server reachable
✓ Provider responding
```
- TTS provider accessible
- Ready to synthesize

**Disconnected:**
```
✗ Connection failed
✗ Provider not responding
```
- Cannot reach TTS provider
- Check provider is running
- Verify network/firewall

### Streaming Status

**Streaming Enabled:**
```
Streaming: Yes
```
- Real-time audio playback
- Low latency (~500ms)
- VibeVoice only

**Streaming Disabled:**
```
Streaming: No
```
- Full audio generated first
- Higher latency (2-3s)
- All providers support this

## Use Cases

### Quick Status Check

```bash
# Before important demo
/tts-status

# Verify:
# - Status: Enabled ✓
# - Provider: Connected ✓
# - Voice: Appropriate ✓
```

### Troubleshooting

```bash
# Audio not working
/tts-status --test

# Check:
# - Provider reachable?
# - Model loaded?
# - Audio device available?
```

### Configuration Verification

```bash
# After making changes
/tts-config --voice nova
/tts-status --verbose

# Verify:
# - Voice updated to nova ✓
# - All settings preserved ✓
```

### Provider Comparison

```bash
# Test VibeVoice
/tts-config --provider vibevoice
/tts-status --test

# Test OpenAI
/tts-config --provider openai
/tts-status --test

# Compare response times and quality
```

## Troubleshooting with Status

### Problem: No Audio

```bash
/tts-status --verbose

# Check:
# 1. Status: Enabled? If not: /tts-enable
# 2. Provider reachable? If not: start provider
# 3. Auto-play: true? If not: /tts-config --playback
```

### Problem: Slow Synthesis

```bash
/tts-status --verbose

# Check:
# 1. Streaming: Yes? If no: enable streaming
# 2. Provider: Local (vibevoice)? Cloud is slower
# 3. Inference steps: <15? Higher = slower
```

### Problem: Poor Quality

```bash
/tts-status --verbose

# Check (VibeVoice):
# 1. CFG Scale: <1.5? Increase for quality
# 2. Inference Steps: <10? Increase for quality
#
# Check (OpenAI):
# 1. Model: tts-1? Switch to tts-1-hd
```

### Problem: Wrong Voice

```bash
/tts-status --verbose

# Check:
# Current voice: Shows what's actually set
#
# Fix:
/tts-config --voice <desired-voice>
/tts-status  # Verify change
```

## Configuration File Reference

Status shows where configuration is stored:

```
Configuration: ~/.claude/settings.json
```

You can manually edit this file or use:
```bash
# View configuration
cat ~/.claude/settings.json | grep -A 30 voice-output

# Edit configuration
vim ~/.claude/settings.json

# Validate configuration
jq . ~/.claude/settings.json
```

## Statistics and Metrics

With `--verbose`, status shows usage statistics:

```
Statistics:
  Total syntheses: 47
  Total audio time: 12m 34s
  Average response time: 1.2s
  Success rate: 98.7%
  Errors: 1 (connection timeout)
```

**Metrics tracked:**
- **Total syntheses**: Number of TTS operations
- **Total audio time**: Cumulative audio duration
- **Average response time**: Mean synthesis latency
- **Success rate**: Percentage of successful syntheses
- **Errors**: Recent error count and types

## Output Formats

### JSON Output

For programmatic use:

```bash
/tts-status --json

# Output:
{
  "enabled": true,
  "provider": "vibevoice",
  "voice": "alloy",
  "streaming": true,
  "health": {
    "status": "healthy",
    "responseTime": 145,
    "serverVersion": "1.0.0"
  },
  "configuration": {
    "vibevoice": {
      "base_url": "http://localhost:5381",
      "cfg_scale": 1.3,
      "inference_steps": 10
    }
  }
}
```

## Related Commands

- `/tts-enable` - Enable automatic TTS
- `/tts-disable` - Disable automatic TTS
- `/tts-config` - Configure TTS settings
- `/speak` - Manual TTS trigger

## See Also

- [Voice Output Plugin README](../README.md)
- [Configuration Guide](../README.md#configuration)
- [Troubleshooting Guide](../README.md#troubleshooting)
