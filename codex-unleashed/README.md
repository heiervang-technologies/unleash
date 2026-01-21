# Codex Unleashed - OpenRouter Configuration

This directory contains configuration for using OpenAI Codex CLI with OpenRouter models.

## Quick Setup

1. Copy the configuration to your Codex config directory:
   ```bash
   cp codex-unleashed/config.toml ~/.codex/config.toml
   ```

2. Set your OpenRouter API key:
   ```bash
   export OPENROUTER_API_KEY="your-api-key-here"
   ```

   Add to your shell profile (`~/.bashrc` or `~/.zshrc`) to make it permanent.

3. Test the configuration:
   ```bash
   codex exec "Hello, test message"
   ```

## Configuration Details

The `config.toml` file includes:

- **Default model**: `z-ai/glm-4.7` - ZhipuAI GLM-4.7 model
- **Minimax profile**: `minimax/minimax-m2.1` - Minimax M2.1 model
- **Provider**: OpenRouter API (`https://openrouter.ai/api/v1`)
- **Wire API**: `responses` (updated format, no deprecation warnings)

## Usage

### Default model (GLM-4.7):
```bash
codex exec "your prompt"
```

### Use Minimax profile:
```bash
codex --profile minimax "your prompt"
```

### Override with any OpenRouter model:
```bash
codex --model "anthropic/claude-sonnet-4-5" "your prompt"
```

## Repository Structure

```
claude-unleashed/
└── codex-unleashed/          # Configuration directory
    ├── codex/                # OpenAI Codex CLI submodule (main branch)
    ├── config.toml           # OpenRouter configuration
    └── README.md             # This file
```

## Available Models

You can use any model from [OpenRouter](https://openrouter.ai/models), including:
- `z-ai/glm-4.7` (default)
- `minimax/minimax-m2.1` (profile included)
- `anthropic/claude-sonnet-4-5`
- `openai/gpt-4-turbo`
- `google/gemini-pro`
- `meta-llama/llama-3.1-70b-instruct`
- And many more...
