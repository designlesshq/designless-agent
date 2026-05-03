# Getting Started

This guide walks you through connecting to Designless and creating your first brand.

## Prerequisites

- Claude Code (v1.0.33+) or Cursor with agent support
- A Designless account — create one at [designless.app](https://designless.app)

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

Run the connect command:
```
> /designless connect
```

The agent detects whether the MCP server is configured. If not, it runs the install automatically:
```bash
claude mcp add --transport http less-mcp https://mcp.designless.app/mcp
```

A browser window opens for OAuth authentication. Complete login at [designless.app](https://designless.app) and return — the session continues automatically.

**Verify the connection:**
```
> /designless status
```

If you see your account tier and capabilities listed, you're connected.

### Get embed snippets for an existing brand

If you've already created a brand and just need to wire it into a new
project, run:

```
> less_init <your-brand-slug>
```

The response includes an `integration` block with framework-specific embed
snippets (HTML, Next.js, Vite, Astro, SvelteKit, Nuxt) and per-platform env-var
setup instructions (Vercel, Netlify, Render, Railway, Supabase). The agent will
copy-paste the right snippet for your stack — no external doc lookup needed.

## Step 3: Create Your First Brand

```
> /designless
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

Once your brand exists, describe what you want:

**Generate a carousel:**
```
> /designless
> 5-slide LinkedIn carousel about why design systems break
> when agents start building UI. Use my brand.
```

**Build a landing page component:**
```
> /designless
> Hero section — dark background, gradient accent, headline + subhead + CTA.
> Responsive, production-ready HTML.
```

**Audit the brand before handing it to engineering:**
```
> /designless audit my brand
```

**Extend with new capabilities:**
```
> /designless
> Add animation tokens — snappy for micro-interactions, smooth for page transitions.
```

## Walkthrough Scenarios

### Scenario 1: Agency Creating Client Brands

You run a design agency and need to produce brand systems for clients quickly.

```
> /designless
> Premium skincare brand. Feminine, luxurious, minimal. Think Aesop meets Glossier.
> Primary audience is 25-40 year old women.
```

Review the output. If you want to shift direction:

```
> /designless
> Make it warmer — the palette feels too clinical. Keep the minimalism
> but add more warmth to the neutrals.
```

Compile and hand off:
```
> /designless
> Run the full quality gate — I'm sharing this with the client.
```

### Scenario 2: Developer Adding Brand to an Existing Project

You have a working app but it looks generic. You want to add real brand character.

```
> /designless
> SaaS dashboard for analytics. Professional but not boring.
> Blue/indigo primary, clean data visualization palette, compact density.
```

Then immediately build with it:
```
> /designless
> Rebuild my sidebar navigation using my brand tokens. Keep the existing
> structure but apply brand colors, typography, and spacing.
```

### Scenario 3: Adopting a Figma Design System

You have an existing design system in Figma and want to bring it into the expression infrastructure.

```
> /designless
> Import my Figma design system. Here's the file: [paste Figma URL or export]
```

The agent analyzes the external system, maps it into the expression infrastructure format, resolves any gaps or conflicts, and creates a brand you can extend and govern going forward.

## Troubleshooting

**"Not authenticated"** — Run `/designless connect` to trigger the OAuth flow and authenticate via your browser.

**"No brands found"** — Run `/designless` and describe a new brand to get started. Most flows need an existing brand.

**"Server unreachable"** — Check your network connection and verify the endpoint at `https://mcp.designless.app/mcp`. The agent needs a live server connection for all capabilities.

**"Capsule not published"** — The brand exists but hasn't been compiled yet. Run `/designless` and ask to publish — the agent walks you through the quality gate.

## Next Steps

- Read [Capabilities](capabilities.md) for a detailed breakdown of everything the agent can do
- Visit [designless.io](https://designless.io) for the full thesis on expression infrastructure
- Explore [designless.live](https://designless.live) for the vocabulary and philosophy behind the project
- Try [designtoken.md](https://designtoken.md) — design token generator
