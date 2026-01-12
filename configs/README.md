# Codex CLI Configuration Examples

This directory contains example configurations for OpenAI Codex CLI with various providers.

## OpenRouter Configuration

`codex-openrouter.toml` - Configuration for using Codex CLI with OpenRouter models

### Setup Instructions

1. Copy the configuration to your Codex config directory:
   ```bash
   cp configs/codex-openrouter.toml ~/.codex/config.toml
   ```

2. Set your OpenRouter API key:
   ```bash
   export OPENROUTER_API_KEY="your-api-key-here"
   ```

   Add this to your shell profile (`~/.bashrc` or `~/.zshrc`) to make it permanent.

3. Test the configuration:
   ```bash
   codex exec "Hello, test message"
   ```

### Available Models

The configuration includes:

- **Default model**: `z-ai/glm-4.7` - ZhipuAI GLM-4.7 model
- **Minimax profile**: `minimax/minimax-m2.1` - Minimax M2.1 model

### Usage

**Default model (GLM-4.7):**
```bash
codex exec "your prompt"
```

**Use Minimax profile:**
```bash
codex --profile minimax "your prompt"
```

**Override with any OpenRouter model:**
```bash
codex --model "anthropic/claude-sonnet-4-5" "your prompt"
```

### Configuration Details

- **Provider**: OpenRouter API (`https://openrouter.ai/api/v1`)
- **Wire API**: `responses` (updated format, no deprecation warnings)
- **Authentication**: Environment variable `OPENROUTER_API_KEY`

### Available OpenRouter Models

You can use any model from [OpenRouter](https://openrouter.ai/models), including:
- `anthropic/claude-sonnet-4-5`
- `openai/gpt-4-turbo`
- `google/gemini-pro`
- `meta-llama/llama-3.1-70b-instruct`
- And many more...
