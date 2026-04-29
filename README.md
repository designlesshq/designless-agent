# Designless Agent

Designless agent — encode your brand's design judgment into an agent that builds with your intent.

## Install

### Plugin (recommended)

Install the Designless plugin in Claude Code:

```bash
claude plugin marketplace add designlesshq/designless-agent
claude plugin install designless@designless-plugins
```

Then reload plugins and start using:

```bash
claude /reload-plugins
claude /designless
```

### MCP server

Add the expression infrastructure runtime directly to any MCP-compatible agent:

```bash
claude mcp add --transport http less-mcp https://mcp.designless.app/mcp
```

This gives your agent access to LESS MCP tools — it will use them when you reference your brand, ask about design tokens, or request brand-consistent output. No `/designless` commands, but the runtime capabilities are available. Authentication is handled via OAuth on first use.

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

After install, the orchestrator is available in your chosen agents. It connects to the expression infrastructure at `mcp.designless.app/mcp` — authentication is handled via OAuth on first use.

## One command, every flow

```
/designless
```

That's it. There's no `/designless:create`, `/designless:audit`, or any other sub-command — the orchestrator detects your intent from what you say and routes to the right capability at runtime. You describe what you want; it figures out whether to create a brand, extend it, adopt an external system, build a page, audit quality, prove provenance, generate a carousel or poster, or surface ecosystem state.

## Quick Start

```
> /designless
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

**"Not authenticated"** — Run `/designless connect` and complete the OAuth flow in your browser.

**"No brands found"** — Run `/designless` and ask for a new brand to get started.

**"Capsule not published"** — Run `/designless` and ask to publish; the agent walks you through the quality gate.

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
