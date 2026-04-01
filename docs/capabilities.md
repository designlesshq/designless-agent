# Capabilities

The Designless agent connects to an expression infrastructure server that encodes design judgment into runtime capabilities. Here's what you can do.

## Expression Creation

Create complete brand expression systems from natural language, visual references, or keywords.

**What you provide:**
- A description ("minimalist fintech brand, trustworthy, geometric, blue tones")
- Or a screenshot or reference image
- Or a set of keywords

**What you get back:**
- A full expression system with 300+ resolved design tokens across 12 categories (colors, typography, layout, spacing, radius, shadows, borders, motion, sizes, z-index, opacity, space)
- An archetype that captures the brand's personality and intent
- Coherence scores measuring how well every token works together
- Decision provenance for every value — not just "blue-600" but why that blue was chosen, what it relates to, and how it fits the whole
- A compiled Brand Capsule — one versioned artifact containing the complete brand, ready for agents to consume

**Example:**
```
> /designless:create
> I need a brand for a developer tools startup — technical but approachable,
> dark mode first, inspired by terminal aesthetics but not cold.
```

The agent creates the entire expression system — 300+ tokens resolved from your description — walks you through the key decisions, compiles a capsule, and publishes it so any agent in your environment can build with it. This is the DLM — the Design Language Model — resolving natural language into deterministic infrastructure.

## Runtime Design System

Serve live design tokens that respond to context — not a static export, but a runtime API.

**What you can do:**
- Push token overrides for specific contexts (dark mode, compact density, high contrast)
- Extend a brand with new token categories (add motion tokens, add illustration styles)
- Evolve a brand over time — shift the palette warmer, modernize the typography, refresh without breaking
- Resolve conflicts when competing design decisions collide

**Example:**
```
> /designless:extend
> Add a set of motion tokens to my brand — transitions should feel snappy
> for interactions but smooth for page-level animations.
```

The agent extends your existing brand with contextually appropriate motion tokens that maintain coherence with your established visual language.

## Brand Governance

Compile, version, audit, and validate brands with quality gates and evidence chains.

**What you can do:**
- Compile a brand into a Brand Capsule — a self-contained, versioned artifact that encodes everything about how the brand should look, feel, and speak
- Run brand health audits covering accessibility (contrast ratios, color blindness), coherence (do all tokens work together?), and quality (does it meet production standards?)
- Publish and rollback capsule versions with full version history
- Prove that generated output is on-brand using traceable evidence — EvidenceKit traces every design decision back to its source token and returns structured proof, not opinion

**Example:**
```
> /designless:audit
> Run a full health check on my brand before I hand it to the engineering team.
```

The agent runs accessibility checks, coherence scoring, and quality gates, then presents a unified audit report with specific findings and recommendations.

**Example:**
```
> /designless:prove
> Prove that this hero section is on-brand. Show me the evidence.
```

The agent traces every design decision in the component back to its source token, checks coherence scores, and returns an evidence chain you can review or share.

## Visual Expression (via Prism)

Create brand-aligned visual content across 20 document types in 6 categories — with every pixel governed by your brand tokens.

**Template Categories:**
- **Social Media** — Instagram carousels, LinkedIn documents, stories, posts, Twitter/X cards, YouTube thumbnails
- **Business** — Pitch decks, sales decks, one-pagers, quarterly reports, brochures
- **Web** — Landing page heroes, blog post headers, display ads (with HTML export)
- **Marketing** — Email templates (with HTML export), posters, promotional flyers
- **Brand** — Identity sheets, brand guidelines, business cards
- **Visual** — Infographics, data visualizations

**What you can do:**
- Generate any of the 20 document types from a topic or outline
- Create branded visual content with platform-specific rules (safe zones, text coverage limits, aspect ratios)
- Export to PNG, PDF, or self-contained HTML depending on document type
- HTML export available for web and email templates — responsive, self-contained, brand-enforced
- All output uses your brand's actual tokens — no hardcoded values, no generic defaults

**Example:**
```
> /designless:carousel
> Create a 5-slide LinkedIn carousel about why design tokens alone aren't enough.
> Use my brand.
```

Prism generates the carousel using your published Brand Capsule — colors, typography, spacing, and voice all come from your expression system. The output includes Composition Quality scores confirming brand alignment.

**Example:**
```
> /designless:build
> Build a landing page hero section for my brand. Dark background,
> headline + subhead + CTA, responsive.
```

Prism generates production HTML where every color, font, spacing value, and shadow comes from your brand tokens. No generic CSS — it's your brand, enforced at the infrastructure level.

## Ecosystem Status

See your full Designless environment at a glance.

```
> /designless:status
```

Returns an overview of your brands, capsule states, API tier, and available capabilities. Useful for orienting before you start working.

---

## How Brands Are Selected

Every command that operates on a brand needs to know which brand to use. The agent handles this automatically:

- **One brand** — auto-selected, no question asked.
- **Multiple brands** — the agent asks which one you want to work with.
- **No brands** — the agent redirects you to `/designless:create` first.

Brand detection happens at the start of every command. You never need to specify a brand slug manually — the agent queries the server for your brand inventory and resolves the right one from context.

---

## What Makes This Different

Traditional design systems give you a file of tokens. You export them, paste them into your codebase, and hope they stay in sync.

Expression infrastructure is a runtime layer. Tokens are resolved live, governed by quality gates, and served to agents at the moment of creation. The agent doesn't read a file — it queries infrastructure that understands context, enforces coherence, and provides decision provenance for every value.

The agent discovers its capabilities from the server at runtime. As the infrastructure evolves, the agent's capabilities grow — no plugin updates required.
