# Capabilities

The Designless agent connects to an expression infrastructure server that encodes design judgment into runtime capabilities. Here is what you can do.

## Create a brand

Create a complete brand expression system from natural language, visual references, or keywords.

**What you provide:**
- A description ("minimalist fintech brand, trustworthy, geometric, blue tones")
- Or a screenshot or reference image
- Or a set of keywords

**What you get back:**
- A full expression system with 300+ resolved design tokens spanning every category your build needs: color, typography, layout, spacing, motion, effects, and the rest
- An archetype that captures the brand's personality and intent
- Coherence scores measuring how well every token works together
- Decision provenance for every value. Not just "blue-600" but why that blue was chosen, what it relates to, and how it fits the whole
- A compiled Brand Capsule: one versioned artifact containing the complete brand, ready for agents to consume

**Example:**
```
> /designless
> I need a brand for a developer tools startup. Technical but approachable,
> dark mode first, inspired by terminal aesthetics but not cold.
```

The agent creates the entire expression system from your description, walks you through the key decisions, compiles a capsule, and publishes it so any agent in your environment can build with it. This is the DLM, the Design Language Model, resolving natural language into deterministic infrastructure.

## Adopt an existing design system

Already have a design system? Bring it in and govern it going forward.

**What you can provide:**
- A Figma file or exported variables
- CSS custom properties or a token file
- A screenshot of a deployed site, or a live URL

**What you get back:**
- A Brand Capsule resolved from your existing system, with compatibility notes wherever your source and Designless diverge
- A brand you can extend, evolve, and audit like any other, no manual re-entry

**Example:**
```
> /designless
> Import my Figma design system. Here is the file: [paste Figma URL or export]
```

The agent reads the external system, maps it into an expression system, resolves gaps or conflicts, and hands you back a brand that is yours to build on.

## Serve a runtime design system

Serve live design tokens that respond to context. Not a static export, a runtime layer.

**What you can do:**
- Push token overrides for specific contexts (dark mode, compact density, high contrast)
- Extend a brand with new token categories (add motion tokens, add illustration styles)
- Evolve a brand over time. Shift the palette warmer, modernize the typography, refresh without breaking
- Resolve conflicts when competing design decisions collide

**Example:**
```
> /designless
> Add a set of motion tokens to my brand. Transitions should feel snappy
> for interactions but smooth for page-level animations.
```

The agent extends your existing brand with contextually appropriate motion tokens that stay coherent with your established visual language.

## Govern a brand

Compile, version, audit, and validate brands with quality gates and evidence chains.

**What you can do:**
- Compile a brand into a Brand Capsule: a self-contained, versioned artifact that encodes everything about how the brand should look, feel, and speak
- Run brand health audits covering accessibility (contrast, color blindness), coherence (do all tokens work together?), and quality (does it meet production standards?)
- Publish and roll back capsule versions with full version history
- Prove that generated output is on-brand using traceable evidence. EvidenceKit ties every design decision back to its source and returns structured proof, not opinion

**Example:**
```
> /designless audit
> Run a full health check on my brand before I hand it to the engineering team.
```

The agent runs accessibility checks, coherence scoring, and quality gates, then presents one unified audit report with specific findings and recommendations.

**Example:**
```
> /designless prove this hero section is on-brand. Show me the evidence.
```

The agent ties every design decision in the component back to the brand, checks coherence, and returns an evidence chain you can review or share.

## Build with a coding agent

Give your coding agent the brand at the moment it builds, so its output carries taste, not just working code.

**What you can do:**
- Check generated code against your brand's rules and catch values that should be brand-governed
- Write copy that speaks in your brand's voice: surface-aware content that matches tone, length, and terminology (ContentKit)
- Validate output against your expression contracts before it ships
- Keep a brand glossary so agents use your language consistently

**Example:**
```
> /designless
> Rebuild my sidebar using my brand tokens. Keep the structure,
> apply brand colors, typography, and spacing, and flag anything hardcoded.
```

The brand is available while the agent works, not pasted into a prompt and lost by the fortieth turn.

## Create visual content (via Prism)

Create brand-aligned visual content across a wide range of document types, with every pixel governed by your brand tokens.

**What you can make:**
- **Social**: carousels, stories, posts, cards, thumbnails, with platform-specific rules (safe zones, text coverage limits, aspect ratios)
- **Business**: pitch decks, sales decks, one-pagers, reports, brochures
- **Web**: landing page heroes, blog headers, display ads
- **Marketing**: email templates, posters, promotional flyers
- **Brand**: identity sheets, guidelines, business cards
- **Visual**: infographics, data visualizations

**What you can do:**
- Generate any of these from a topic or an outline
- Export to PNG, PDF, or self-contained HTML depending on the document type
- See it render live on the Designless canvas, edit it interactively, then export
- Every output uses your brand's actual tokens. No hardcoded values, no generic defaults

**Example:**
```
> /designless
> Create a 5-slide LinkedIn carousel about why design tokens alone aren't enough.
> Use my brand.
```

Prism generates the carousel from your published Brand Capsule. Colors, typography, spacing, and voice all come from your expression system. The output carries Composition Quality scores confirming brand alignment.

**Example:**
```
> /designless
> Build a landing page hero for my brand. Dark background,
> headline + subhead + CTA, responsive.
```

Prism generates production HTML where every color, font, spacing value, and shadow comes from your brand tokens. No generic CSS. It is your brand, enforced at the infrastructure level.

## Build in your own app (via Prism)

Point Designless at your own running app and edit it on the canvas, with your brand enforced as you go.

**What you can do:**
- Bring a running app or local project onto the Designless canvas
- Make visual edits and see them applied against your brand's tokens
- Keep the work traceable, so every change ties back to a brand decision

This runs in the Designless desktop app, where the live render and your edits stay in sync. When you are done, promote the result to your repo (below).

## Collaborate with your team (via Prism)

Bring your team into a page-mode session. Share the session, and teammates propose visual edits while you stay in control of what ships.

**What you can do:**
- Share a page-mode session with your team, explicitly and per session. Nothing is visible to anyone until you share it.
- Teammates review the live render and propose edits against your brand.
- You promote the result to your repo. The promote step stays with you.
- Comment threads keep the discussion anchored to the exact spot on the work.

This is async collaboration: your team proposes, you promote. Available on Team plans and above.

## Promote to production (via Prism)

When you are ready to ship the edits you made to your own app on the canvas, promote them to your repo, as a pull request for review or merged after your required checks pass. It runs in your own checkout with your own `gh`/`git`. Designless never holds a GitHub token or touches your repo directly.

**Example:**
```
> /designless
> Looks good, ship it.
```

The agent shows you exactly what will be promoted, then asks whether to open a PR, merge, or hold off. It never force-pushes and never pushes straight to your default branch.

## Ecosystem status

See your full Designless environment at a glance.

```
> /designless status
```

Returns an overview of your brands, capsule states, plan tier, and available capabilities. Useful for orienting before you start working.

---

## How brands are selected

Every command that operates on a brand needs to know which brand to use. The agent handles this automatically:

- **One brand**: auto-selected, no question asked.
- **Multiple brands**: the agent asks which one you want to work with.
- **No brands**: the agent prompts you to describe a new brand, or adopt an existing one, first.

Brand detection happens at the start of every command. You never need to specify a brand slug by hand. The agent resolves the right brand from your inventory and your context.

---

## What makes this different

Traditional design systems give you a file of tokens. You export them, paste them into your codebase, and hope they stay in sync.

Expression infrastructure is a runtime layer. Tokens are resolved live, governed by quality gates, and served to agents at the moment of creation. The agent does not read a file. It queries infrastructure that understands context, enforces coherence, and provides decision provenance for every value.

The agent discovers its capabilities from the server at runtime. As the infrastructure grows, the agent's capabilities grow with it, no plugin updates required.
