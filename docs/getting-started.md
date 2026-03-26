# Getting Started

This guide walks you through connecting to Designless and creating your first brand.

## Prerequisites

- Claude Code (v1.0.33+) or Cursor with agent support
- A Designless API key (free — get one at [build.designless.studio](https://build.designless.studio))

## Step 1: Install the Plugin

**Claude Code:**
```bash
claude plugin marketplace add designlesshq/designless-agent
claude plugin install designless@designless-plugins
```

**Cursor:**
```
/add-plugin designless
```

After installing, run `/reload-plugins` to activate.

## Step 2: Connect to the Expression Infrastructure

Set your API key as an environment variable:
```bash
export LESS_API_KEY=your_key_here
```

Or add the MCP server directly:
```bash
claude mcp add --transport http designless \
  --header "x-api-key: YOUR_KEY" \
  https://mcp.designless.studio/mcp
```

**Verify the connection:**
```
> /designless:status
```

If you see your account tier and capabilities listed, you're connected. If you see an auth error, check that your API key is set correctly.

## Step 3: Create Your First Brand

```
> /designless:create
> A developer tools brand — modern, technical, dark mode first.
> Think terminal aesthetics but warm, not cold.
```

The agent will:
1. Analyze your description and extract design intent
2. Resolve a complete expression system (colors, typography, spacing, effects, voice)
3. Show you the key decisions with reasoning for each
4. Compile a Brand Capsule — a versioned artifact encoding the entire brand
5. Publish the capsule so other agents can consume it

This typically takes 30-60 seconds. At the end, you have a production-ready brand with 300+ tokens across 12 categories, coherence scores, and full decision provenance.

## Step 4: Build Something With It

Once your brand exists, use it:

**Generate a carousel:**
```
> /designless:carousel
> 5-slide LinkedIn carousel about why design systems break
> when agents start building UI.
```

**Build a landing page component:**
```
> /designless:build
> Hero section — dark background, gradient accent, headline + subhead + CTA.
> Responsive, production-ready HTML.
```

**Audit the brand before handing it to engineering:**
```
> /designless:audit
```

**Extend with new capabilities:**
```
> /designless:extend
> Add animation tokens — snappy for micro-interactions, smooth for page transitions.
```

## Walkthrough Scenarios

### Scenario 1: Agency Creating Client Brands

You run a design agency and need to produce brand systems for clients quickly.

```
> /designless:create
> Premium skincare brand. Feminine, luxurious, minimal. Think Aesop meets Glossier.
> Primary audience is 25-40 year old women.
```

Review the output. If you want to shift direction:

```
> /designless:evolve
> Make it warmer — the palette feels too clinical. Keep the minimalism
> but add more warmth to the neutrals.
```

Compile and hand off:
```
> /designless:audit
> Run the full quality gate — I'm sharing this with the client.
```

### Scenario 2: Developer Adding Brand to an Existing Project

You have a working app but it looks generic. You want to add real brand character.

```
> /designless:create
> SaaS dashboard for analytics. Professional but not boring.
> Blue/indigo primary, clean data visualization palette, compact density.
```

Then immediately build with it:
```
> /designless:build
> Rebuild my sidebar navigation using my brand tokens. Keep the existing
> structure but apply brand colors, typography, and spacing.
```

### Scenario 3: Adopting a Figma Design System

You have an existing design system in Figma and want to bring it into the expression infrastructure.

```
> /designless:adopt
> Import my Figma design system. Here's the file: [paste Figma URL or export]
```

The agent analyzes the external system, maps it into the expression infrastructure format, resolves any gaps or conflicts, and creates a brand you can extend and govern going forward.

## Troubleshooting

**"Missing API key"** — Set `LESS_API_KEY` in your environment, or pass it directly when adding the MCP server with `--header "x-api-key: YOUR_KEY"`.

**"No brands found"** — Run `/designless:create` first. Most commands need an existing brand to work with.

**"Server unreachable"** — Check your network connection and verify the endpoint at `https://mcp.designless.studio/mcp`. The agent needs a live server connection for all capabilities.

**"Capsule not published"** — The brand exists but hasn't been compiled into a capsule yet. Run `/designless:create` to compile and publish, or `/designless:audit` to check the brand's state.

## Next Steps

- Read [Capabilities](capabilities.md) for a detailed breakdown of everything the agent can do
- Visit [designless.studio](https://designless.studio) for the full thesis on expression infrastructure
- Explore [designless.live](https://designless.live) for the vocabulary and philosophy behind the project
- Try [designtoken.md](https://designtoken.md) for a free design token generator
