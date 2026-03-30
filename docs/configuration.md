# Configuration

Unleash uses a minimal global config file. Most settings live in
[profiles](profiles.md) instead.

## Config File

**Location:** `~/.config/unleash/config.toml`

```toml
current_profile = "claude"   # Which profile loads by default
animations = false            # TUI animations (lava color cycling)
```

That's it -- just two fields. Everything else (agent path, theme, flags,
environment variables, plugin settings) is configured per-profile.

## Directory Layout

```
~/.config/unleash/
├── config.toml              # Global config (this file)
└── profiles/                # One TOML file per profile
    ├── claude.toml
    ├── codex.toml
    ├── gemini.toml
    └── opencode.toml
```

## Changing the Default Profile

The TUI updates `current_profile` automatically when you select a profile.
You can also edit it directly:

```bash
# Switch default to codex
sed -i 's/current_profile = .*/current_profile = "codex"/' \
  ~/.config/unleash/config.toml
```

Or launch a specific profile without changing the default:

```bash
unleash codex
```

## Animations

Enable TUI color cycling animations either in the config file or via
environment variable:

```toml
animations = true
```

```bash
UNLEASH_ANIMATIONS=1 unleash
```

See [environment-variables.md](environment-variables.md) for all env vars.

## Legacy Migration

Older versions stored `claude_path`, `claude_args`, and `theme` directly in
`config.toml`. On first run, unleash automatically migrates these fields into
the appropriate profile TOML file and removes them from `config.toml`.

No manual action is needed -- the migration is silent and non-destructive.

## Next Steps

- [Profiles](profiles.md) -- per-agent configuration (paths, flags, themes, env)
- [Environment Variables](environment-variables.md) -- all env vars
- [Plugins](plugins.md) -- bundled plugin index
