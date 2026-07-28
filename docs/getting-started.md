# Getting Started

This guide walks you through connecting to Designless and creating your first brand.

## Prerequisites

- Claude Code (v1.0.33+), Cursor, or any other agent with skill or MCP support
- A Designless account. Create one at [designless.app](https://designless.app)

## Step 1: Install the plugin

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

## Step 2: Connect to the expression infrastructure

How you connect depends on your setup.

**With the Designless desktop app (recommended).** Install the [Designless app](https://designless.app) and sign in. The plugin connects through it automatically, so there is no separate sign-in step. Verify with:
```
> /designless status
```

**Without the desktop app** (a remote or sandbox environment, or any other MCP client). Connect over HTTP and authenticate in your browser:
```bash
claude mcp add --transport http less-mcp https://mcp.designless.app/mcp
```
On first use, a browser opens so you can sign in and authorize access at [designless.app](https://designless.app). Once you approve, the connection completes automatically. Spec-compliant MCP clients discover this flow on their own from the server.

**Verify the connection:**
```
> /designless status
```

If you see your account tier and capabilities listed, you are connected.

### Wire an existing brand into a new project

Already created a brand and just need to embed it in a new project? Ask the agent:

```
> /designless
> Give me the embed snippet to wire my brand into this project.
```

The agent returns the right framework-specific snippet for your stack (HTML, Next.js, Vite, Astro, SvelteKit, Nuxt) plus per-platform environment setup (Vercel, Netlify, Render, Railway, Supabase). It copy-pastes the one that fits, no external doc lookup needed.

## Step 3: Create your first brand

```
> /designless
> A developer tools brand. Modern, technical, dark mode first.
> Think terminal aesthetics but warm, not cold.
```

The agent will:
1. Read your description and extract design intent
2. Resolve a complete expression system (color, typography, spacing, effects, voice)
3. Show you the key decisions with reasoning for each
4. Compile a Brand Capsule, a versioned artifact encoding the entire brand
5. Publish the capsule so other agents can consume it

At the end, you have a production-ready brand with 300+ resolved tokens, coherence scores, and full decision provenance.

## Step 4: Build something with it

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
> Hero section. Dark background, gradient accent, headline + subhead + CTA.
> Responsive, production-ready HTML.
```

**Audit the brand before handing it to engineering:**
```
> /designless audit my brand
```

**Extend with new capabilities:**
```
> /designless
> Add animation tokens. Snappy for micro-interactions, smooth for page transitions.
```

## Walkthrough scenarios

### Scenario 1: Agency creating client brands

You run a design agency and need to produce brand systems for clients quickly.

```
> /designless
> Premium skincare brand. Feminine, luxurious, minimal. Think Aesop meets Glossier.
> Primary audience is 25-40 year old women.
```

Review the output. If you want to shift direction:

```
> /designless
> Make it warmer. The palette feels too clinical. Keep the minimalism
> but add more warmth to the neutrals.
```

Compile and hand off:
```
> /designless
> Run the full quality gate. I'm sharing this with the client.
```

### Scenario 2: Developer adding a brand to an existing project

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

### Scenario 3: Adopting a Figma design system

You have an existing design system in Figma and want to bring it into the expression infrastructure.

```
> /designless
> Import my Figma design system. Here is the file: [paste Figma URL or export]
```

The agent reads the external system, maps it into an expression system, resolves any gaps or conflicts, and creates a brand you can extend and govern going forward.

### Scenario 4: Editing your own app on the canvas

You want to make visual changes to your running app with your brand enforced, then ship them.

```
> /designless
> Show my Next app on the canvas so I can edit the pricing page against my brand.
```

The agent brings your app onto the Designless canvas (in the Designless desktop app), applies your brand as you edit, and keeps every change traceable. When you are done:

```
> /designless
> Looks good, ship it.
```

The agent shows you exactly what will be promoted and asks whether to open a PR or merge, running in your own checkout with your own `gh`/`git`.

## Troubleshooting

**"Not authenticated".** Run `/designless connect` to trigger the sign-in flow and authenticate in your browser.

**"No brands found".** Run `/designless` and describe a new brand, or adopt an existing one, to get started. Most flows need a brand.

**"Server unreachable".** Check your network connection and verify the endpoint at `https://mcp.designless.app/mcp`. The agent needs a live server connection for all capabilities.

**"Capsule not published".** The brand exists but has not been compiled yet. Run `/designless` and ask to publish. The agent walks you through the quality gate.

## Next steps

- Read [Capabilities](capabilities.md) for a detailed breakdown of everything the agent can do
- Visit [designless.io](https://designless.io) for the full thesis on expression infrastructure
- Explore [designless.live](https://designless.live) for the vocabulary and philosophy behind the project
- Try [designtoken.md](https://designtoken.md), the design token generator
