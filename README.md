# Designless Agent

Designless agent — encode your brand's design judgment into an agent that builds with your intent.

## Install

### Plugin (recommended)

Install the Designless plugin in Claude Code:

```bash
claude plugin marketplace add designlesshq/designless-agent
claude plugin install designless@designless-plugins
```

Then reload plugins and start using commands:

```bash
claude /reload-plugins
claude /designless:create
```

### MCP server

Add the expression infrastructure runtime directly to any MCP-compatible agent:

```bash
claude mcp add --transport http designless \
  --header "x-api-key: YOUR_KEY" \
  https://mcp.designless.app/mcp
```

This gives your agent access to LESS MCP tools — it will use them when you reference your brand, ask about design tokens, or request brand-consistent output. No `/designless` commands, but the runtime capabilities are available.

Get an API key at [designless.app](https://designless.app).

### Skills (any coding agent)

Install via [skills.sh](https://skills.sh) to use the Designless orchestrator in any supported coding agent — Cursor, Cline, Codex, Amp, Windsurf, and 40+ others:

```bash
npx skills add designlesshq/designless-agent
```

The installer will:
1. Clone the repo and detect the `designless-orchestrator` skill
2. Ask which agents to install to (Cursor, Cline, Codex, etc.)
3. Choose scope — **Project** (current directory) or **Global** (all projects)
4. Copy the skill into each agent's `.agents/skills/` directory

After install, the orchestrator is available in your chosen agents. It connects to the expression infrastructure at `mcp.designless.app/mcp` — you'll need an API key from [designless.app](https://designless.app) on first use.

## Commands

| Command | What it does |
|---------|-------------|
| `/designless` | Routes to the right mode based on your intent |
| `/designless:create` | Create a new brand or compose a page with an existing one |
| `/designless:extend` | Extend or modify an existing brand's tokens |
| `/designless:adopt` | Import an external design system (Figma, CSS, etc.) |
| `/designless:carousel` | Generate a branded carousel via Prism |
| `/designless:poster` | Generate a branded poster via Prism |
| `/designless:build` | Generate production HTML with full brand enforcement |
| `/designless:audit` | One-shot brand health check — accessibility, coherence, quality |
| `/designless:evolve` | Evolve and refresh an existing brand |
| `/designless:prove` | Evidence-based quality validation — prove something is on-brand |
| `/designless:status` | Ecosystem overview — brands, capsules, tier, capabilities |

## Quick Start

```
> /designless:create
> I want a fintech brand — trustworthy, modern, clean.
```

The agent creates a complete brand expression system from your description — 300+ resolved tokens across 12 categories, coherence scores, and an expression brief ready for building. Every token carries decision provenance: not just a value, but the reasoning behind it.

## How It Works

1. `/designless` detects your context (brand inventory, capsule state, assets provided)
2. Classifies your intent into one of 12 lifecycle modes
3. Discovers and sequences the right capabilities at runtime
4. Returns quality-gated results with coherence metrics

The agent discovers capabilities from the expression infrastructure server at runtime — it doesn't hardcode tool names. This means the agent's capabilities grow as the server evolves.

## Troubleshooting

**"Missing API key"** — Set `LESS_API_KEY` environment variable or pass via `--header` when adding the MCP server.

**"No brands found"** — Run `/designless:create` first to create your first brand.

**"Capsule not published"** — Use `/designless:create` to compile and publish your brand. The agent will guide you through the quality gate.

## Documentation

- [Getting Started](docs/getting-started.md) — Install, connect, and create your first brand
- [Capabilities](docs/capabilities.md) — What the agent can do, with concrete examples
- [Vocabulary](docs/vocabulary.md) — The language of expression infrastructure

## Learn More

- [designless.io](https://designless.io) — Product overview and thesis
- [designless.app](https://designless.app) — LESS Studio (get your API key here)
- [designless.live](https://designless.live) — Vocabulary authority and philosophical depth
- [skill.design](https://skill.design) — Skill designer and registry
- [designtoken.md](https://designtoken.md) — Design token generator
