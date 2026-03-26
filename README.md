# Designless Agent

Expression infrastructure agent — encode your brand's design judgment into an agent that builds with your intent.

## Install

```bash
claude mcp add --transport http designless \
  --header "x-api-key: YOUR_KEY" \
  https://mcp.designless.studio/mcp
```

Get an API key at [designless.studio](https://designless.studio).

## Commands

| Command | Mode | What it does |
|---------|------|-------------|
| `/designless` | Auto-detect | Routes to the right mode based on context |
| `/designless:create` | 01/02 | Create a new brand or compose a new page |
| `/designless:extend` | 03/04 | Extend LESS-generated code or adopt external HTML |
| `/designless:adopt` | 04 | Explicitly adopt third-party HTML into the ecosystem |
| `/designless:carousel` | 05 | Generate a branded carousel via Prism |
| `/designless:poster` | 05 | Generate a branded poster via Prism |
| `/designless:build` | 06 | Generate production HTML from Prism canvas |
| `/designless:audit` | 07 | One-shot brand compliance health check |
| `/designless:evolve` | 08 | Evolve a brand (capsule diff + migration) |
| `/designless:scan` | Security | Run security scan on project code |
| `/designless:prove` | 12 | Show certification chain and provenance |
| `/designless:status` | Overview | Ecosystem status and quality metrics |

## Quick Start

```
> /designless:create
> I want a fintech brand — trustworthy, modern, clean.
```

The agent creates a brand capsule with resolved tokens, compiles expression presets, and publishes. Use `less_init` to get the expression brief for any subsequent generation.

## How It Works

1. `/designless` detects your context (brand inventory, capsule state, assets provided)
2. Classifies into one of 12 lifecycle modes
3. Sequences the right MCP tool calls for that mode
4. Returns quality-gated results with containment scores

## Troubleshooting

**"Missing API key"** — Set `LESS_API_KEY` environment variable or pass via `--header`.

**"No brands found"** — Run `/designless:create` first to create your first brand.

**"Capsule not published"** — Run `less_capsule_compile` then `less_capsule_publish` to publish your capsule.
