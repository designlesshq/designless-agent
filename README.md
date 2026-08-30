# Designless Agent

[![Version](https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Fraw.githubusercontent.com%2Fdesignlesshq%2Fdesignless-agent%2Fmain%2F.claude-plugin%2Fplugin.json&query=%24.version&label=version&color=2C4BC8)](https://github.com/designlesshq/designless-agent/releases)
[![License](https://img.shields.io/badge/license-source--visible%20proprietary-4A4E54)](LICENSE)
[![Claude Code](https://img.shields.io/badge/Claude%20Code-plugin-D97757)](https://docs.claude.com/en/docs/claude-code)
[![Codex](https://img.shields.io/badge/Codex-plugin-10A37F)](https://chatgpt.com)
[![Cursor](https://img.shields.io/badge/Cursor-via%20desktop%20app-6E56CF)](https://designless.app)
[![MCP](https://img.shields.io/badge/MCP-server-F59E0B)](https://modelcontextprotocol.io)

Designless agent: encode your brand's design judgment into an agent that builds with your intent.

## Source-visible, not open source

This repository is public so you can read what runs on your machine: the plugin prose, the hooks, and the bridge source the shipped binary is built from. It is not open source. The code is proprietary to Designless, and modification, redistribution, or derivative works require written consent. See [LICENSE](LICENSE). For the data-flow and privacy posture, see [SECURITY.md](SECURITY.md).

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

Or in the ChatGPT app (Codex): open **Plugins → Add → Add plugin marketplace**, enter `designlesshq/designless-agent`, and install **Designless**. From a terminal:

```bash
codex plugin marketplace add designlesshq/designless-agent
codex plugin add designless@designless-plugins
```

**Cursor:** the Designless desktop app installs and updates the plugin for you; no manual step in Cursor. See Troubleshooting for where to get the app.

### MCP server

Add the expression infrastructure runtime directly to any MCP-compatible agent:

```bash
claude mcp add --transport http less-mcp https://mcp.designless.app/mcp
```

This gives your agent access to LESS MCP tools; it will use them when you reference your brand, ask about design tokens, or request brand-consistent output. No `/designless` commands, but the runtime capabilities are available. Authentication is handled via OAuth on first use.

### Skills (any coding agent)

Install via [skills.sh](https://skills.sh) to use the Designless orchestrator in any supported coding agent: Cursor, Cline, Codex, Amp, Windsurf, and 40+ others.

```bash
npx skills add designlesshq/designless-agent
```

The installer will:
1. Clone the repo and detect the orchestrator skill
2. Ask which agents to install to (Cursor, Cline, Codex, etc.)
3. Choose scope: **Project** (current directory) or **Global** (all projects)
4. Copy the skill into each agent's `.agents/skills/` directory

After install, the orchestrator is available in your chosen agents. It connects to the expression infrastructure at `mcp.designless.app/mcp`; authentication is handled via OAuth on first use.

## One command, every flow

```
/designless
```

That's it. There's no `/designless:create`, `/designless:audit`, or any other sub-command. The orchestrator detects your intent from what you say and routes to the right capability at runtime. You describe what you want; it figures out whether to create a brand, extend it, adopt an external system, build a page, audit quality, prove provenance, generate a carousel or poster, or surface ecosystem state. Visual work renders live on the Designless canvas, where you edit it against your brand; you can even bring your own running app onto the canvas and ship the edits back to your repo as a pull request.

## Quick Start

```
> /designless
> I want a fintech brand: trustworthy, modern, clean.
```

The agent creates a complete brand expression system from your description: resolved tokens spanning every category your build needs, coherence scores, and an expression brief ready for building. Every token carries decision provenance, not just a value but the reasoning behind it.

## How It Works

1. `/designless` detects your context (brand inventory, capsule state, assets provided)
2. Classifies your intent and picks the lifecycle mode that fits
3. Discovers and sequences the right capabilities at runtime
4. Returns quality-gated results with coherence metrics

The agent discovers capabilities from the expression infrastructure server at runtime, routing by intent rather than by a fixed catalogue. This means the agent's capabilities grow as the server evolves.

## Troubleshooting

**"Not authenticated"**: run `/designless connect` and complete the OAuth flow in your browser.

**"No brands found"**: run `/designless` and ask for a new brand to get started.

**"Capsule not published"**: run `/designless` and ask to publish; the agent walks you through the quality gate.

**The canvas never opens, or you installed the plugin on its own**: the canvas renders in the Designless desktop app, which is a separate install. Sign in at [designless.app](https://designless.app), or create an account, and download the app from the user menu; it is not offered on the signed-out page. Open the app and sign in there, then run `/designless` again.

## Documentation

- [Getting Started](docs/getting-started.md): install, connect, and create your first brand
- [Capabilities](docs/capabilities.md): what the agent can do, with concrete examples
- [Vocabulary](docs/vocabulary.md): the language of expression infrastructure

## Learn More

- [designless.io](https://designless.io): product overview and thesis
- [designless.app](https://designless.app): LESS Studio, where you sign in and manage your workspace
- [designless.live](https://designless.live): vocabulary authority and philosophical depth
