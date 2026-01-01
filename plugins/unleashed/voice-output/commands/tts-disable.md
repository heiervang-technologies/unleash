# tts-disable - Disable Automatic Text-to-Speech

Disable automatic text-to-speech for Claude responses.

## Usage

```bash
/tts-disable          # Disable auto-TTS
/tts-disable --keep-config  # Disable but preserve settings
```

## Description

The `tts-disable` command turns off automatic text-to-speech synthesis for Claude responses. After disabling, you can still use manual TTS with the `/speak` command.

This is useful for:
- Switching to silent reading mode
- Reducing battery usage on laptops
- Working in noise-sensitive environments
- Temporarily disabling without losing configuration

## Options

- **`--keep-config`** - Disable TTS but preserve all settings (default behavior)
- **`--reset`** - Disable and reset to default configuration

## Behavior

When automatic TTS is disabled:

1. **PostToolUse hook** still runs but skips TTS synthesis
2. **Configuration is preserved** for easy re-enabling
3. **Manual TTS** (`/speak`) continues to work
4. **Settings remain** in `.claude/settings.json`

## Examples

### Basic Disable

```bash
/tts-disable

# Output:
# ✓ Automatic TTS disabled
# Manual TTS still available with /speak command
# To re-enable: /tts-enable
```

### Disable with Configuration Preserved

```bash
# Disable but keep all settings
/tts-disable --keep-config

# Later, re-enable with same settings
/tts-enable

# Same voice, provider, and settings as before
```

### Disable and Reset

```bash
# Disable and reset to defaults
/tts-disable --reset

# All custom settings removed
# Next /tts-enable uses defaults
```

## What Changes

### Settings File Update

The command updates `.claude/settings.json`:

**Before:**
```json
{
  "plugins": {
    "voice-output": {
      "enabled": true,
      "provider": "vibevoice",
      "vibevoice": {
        "voice": "echo"
      }
    }
  }
}
```

**After:**
```json
{
  "plugins": {
    "voice-output": {
      "enabled": false,
      "provider": "vibevoice",
      "vibevoice": {
        "voice": "echo"
      }
    }
  }
}
```

Notice: Only `enabled` changes from `true` to `false`. All other settings preserved.

## What Still Works

After disabling automatic TTS, these features remain available:

### Manual TTS
```bash
# Still works
/speak "Hello world"
/speak --voice nova "Test"
/speak --save output.wav "Save audio"
```

### Configuration Commands
```bash
# Still works
/tts-config
/tts-status
/tts-enable  # Re-enable
```

### Provider Testing
```bash
# Still works
/speak --provider openai "Test OpenAI"
/speak --provider elevenlabs "Test ElevenLabs"
```

## Re-enabling

To turn automatic TTS back on:

```bash
# Simple re-enable
/tts-enable

# All previous settings restored:
# - Same provider
# - Same voice
# - Same filtering rules
# - Same playback settings
```

## Use Cases

### Temporary Silence

```bash
# Reading in a library
/tts-disable

# Later, at home
/tts-enable
```

### Battery Saving

```bash
# On laptop battery
/tts-disable

# Plugged in again
/tts-enable
```

### Context Switching

```bash
# Quiet work environment
/tts-disable

# Commute / driving (hands-free)
/tts-enable
```

### Meeting Preparation

```bash
# Before meeting (silent prep)
/tts-disable

# After meeting (resume audio)
/tts-enable
```

## Comparison: Disable vs Reset

### Disable (Default)
```bash
/tts-disable
```
- Sets `enabled: false`
- Preserves all settings
- Quick re-enable with same config
- **Recommended** for temporary disabling

### Reset
```bash
/tts-disable --reset
```
- Sets `enabled: false`
- Removes custom settings
- Reverts to defaults
- **Use** when starting fresh

## Troubleshooting

### TTS Still Playing After Disable

**Check:**
```bash
/tts-status
```

**Should show:**
```
Status: Disabled
Provider: vibevoice (configured but not active)
```

**Solutions:**
1. Verify command succeeded: `/tts-disable`
2. Check settings file: `cat ~/.claude/settings.json | grep -A 5 voice-output`
3. Restart Claude Code if needed: `/restart`

### Manual TTS Not Working After Disable

This is expected behavior. Manual `/speak` command should still work.

**Check:**
```bash
/speak "Test"
```

**If not working:**
1. Verify provider is running (VibeVoice: `curl http://localhost:5381/health`)
2. Check API keys (OpenAI/ElevenLabs)
3. Review logs: `tail -f ~/.claude/logs/voice-output.log`

### Settings Not Preserved

**Check:**
```bash
# Before disable
/tts-status

# After disable
/tts-status

# After re-enable
/tts-status
```

All three should show same provider, voice, etc. (only enabled status differs).

**If settings lost:**
1. Don't use `--reset` flag
2. Check settings file permissions: `ls -la ~/.claude/settings.json`
3. Verify settings file valid JSON: `jq . ~/.claude/settings.json`

## Configuration

The disable command respects these settings:

```json
{
  "plugins": {
    "voice-output": {
      "enabled": false,
      "provider": "vibevoice",
      "vibevoice": {
        "voice": "alloy"
      },
      "playback": {
        "auto_play": true,
        "save_to_file": false
      },
      "filtering": {
        "skip_code_blocks": true,
        "max_length": 4096
      }
    }
  }
}
```

Only `enabled` is modified by `/tts-disable`.

## Quick Toggle Workflow

```bash
# Quick on/off toggle
/tts-enable    # Turn on
# ... work with audio ...
/tts-disable   # Turn off
# ... work silently ...
/tts-enable    # Turn on again

# All settings preserved throughout
```

## Performance Impact

Disabling TTS:
- **Immediately stops** audio synthesis
- **Frees** TTS provider resources (if streaming)
- **Reduces** network usage (if using cloud providers)
- **No overhead** - PostToolUse hook exits early

Re-enabling TTS:
- **Instant** - just toggles a flag
- **No re-initialization** needed
- **Same latency** as before

## Privacy Considerations

When disabled:
- **No text sent** to TTS providers
- **No audio generated** or stored
- **Manual `/speak`** still sends text to provider
- **Settings preserved** locally in settings file

## Related Commands

- `/tts-enable` - Enable automatic TTS
- `/speak` - Manual TTS trigger (still works when disabled)
- `/tts-config` - Configure TTS settings
- `/tts-status` - Show current configuration

## Technical Details

### Hook Behavior

When disabled, the PostToolUse hook:
1. Checks `enabled` setting
2. If `false`, exits immediately
3. No TTS synthesis performed
4. Minimal overhead (~1ms)

### State Management

```python
# Pseudo-code
def post_tool_use_hook(response):
    config = load_config()
    if not config.get("enabled", False):
        return  # Exit early

    # TTS synthesis only if enabled
    synthesize_speech(response)
```

### File System

No file changes except `.claude/settings.json`:
- Audio files not deleted
- Cache remains intact
- Logs preserved
- Provider configs unchanged

## Best Practices

1. **Use disable** for temporary silencing (preserves settings)
2. **Use reset** only when starting fresh
3. **Keep manual TTS** available for important responses
4. **Monitor provider** - disable if provider has issues
5. **Save battery** - disable on laptops when on battery

## Examples by Scenario

### Daily Workflow

```bash
# Morning: Start with audio (commute)
/tts-enable

# Arrive at office: Disable (quiet environment)
/tts-disable

# Lunch break: Enable for podcast-style listening
/tts-enable

# Afternoon: Disable (focused work)
/tts-disable

# Evening: Enable (hands-free while cooking)
/tts-enable
```

### Development Workflow

```bash
# Coding session: Disable (focus)
/tts-disable

# Code review: Manual TTS for specific responses
/speak "Explain this function"

# Documentation: Enable (listen while formatting)
/tts-enable

# Testing: Disable (need silence)
/tts-disable
```

## See Also

- [Voice Output Plugin README](../README.md)
- [TTS Enable Command](./tts-enable.md)
- [TTS Status Command](./tts-status.md)
