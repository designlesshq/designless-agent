# Vocabulary

D\ introduces a new vocabulary because it occupies a new category. These terms are not jargon — they are the precise names for ideas that did not exist before expression infrastructure did.

For full definitions, see the [Designless glossary](https://designless.live/glossary).

## Expression Infrastructure

The foundational layer that makes design judgment available to AI agents as a runtime service. Not a design tool. Not a component library. Infrastructure — like an API, but for taste.

Three quadrants of interface tooling were full: human + templated (page builders), human + composable (design systems), agent + templated (AI builders). D\ fills the fourth: agent + composable. This category did not exist before Designless.

[Full definition →](https://designless.live/vocabulary/expression-infrastructure)

## LESS MCP

Layered Expression Style Standard — expression infrastructure served as a runtime MCP server. Agents query LESS MCP for design decisions the same way they query any other tool server. Token output is W3C DTCG format — structured, typed, semantic. Every token carries decision provenance: not just a value, but the reasoning behind it.

The term is "LESS MCP" — it is a runtime protocol, not a file format.

## The Designless Agent and LoomX

The Designless agent is the user-facing orchestrator — it receives your intent, classifies it into the right lifecycle mode, and sequences the capabilities needed to fulfill it. Under the hood, LoomX is the expression engine that powers the agent's design judgment. Silent by design, LoomX intervenes at the moment a design decision is made — not with a constraint that prevents action, but with guidance that shapes how the agent acts. The output is the agent's. The judgment that shaped it belongs to the brand.

LoomX does not surface a separate interface. It does not ask developers to pause and consult a design system. It is present at the moment of decision without being in the way of the decision.

[Full definition →](https://designless.live/vocabulary/what-is-loomx)

## Brand Capsule

A self-contained compilation of everything that defines how a brand should look, feel, and speak. One artifact. Versioned. Deterministic. When agents consume a capsule, they produce brand-consistent output automatically. A brand's DNA — compressed into a single, immutable package any machine can read and any human can trust.

## The Expression Gap

The gap between what you express and what gets built. You prompt your vision. You get back generic components. By prompt forty, your tokens are burned. Expression infrastructure resolves this gap permanently — your brand is available at runtime, not pasted into a prompt.

## Expression Contracts

Design tokens name values — `color.primary`, `space.lg`. Expression contracts encode intent: what the primary color means, the contexts in which it carries weight, how it relates to the brand's voice. Tokens are a vocabulary list. Contracts are a fluent speaker. Tokens were built for engineers. Contracts are built for agents.

[Full definition →](https://designless.live/vocabulary/design-tokens-vs-expression-contracts)

## Taste as Infrastructure

The company thesis — not a tagline. Taste is the accumulated weight of human judgment about what feels right. It makes a typeface feel inevitable at a size, spacing breathe, interfaces feel made for humans. Taste can be encoded, structured, and served as infrastructure to agents — the same way logic and data are. It is not a soft attribute. It is an addressable layer in the agent stack.

## Brand Operating System

The governing layer that compiles, validates, and distributes a brand's expression identity. Ensures every surface an agent touches speaks with the same intention. A brand guideline is a document people read. A Brand OS is infrastructure machines run.

## Bidirectional Expression Resolver

The capability that resolves brand expression in both directions: from brand intent to design output, and from existing design output back to brand intent. It closes the loop between how a brand means to look and how it actually appears. Not aesthetic review — structured validation with a traceable chain.

## Design Intelligence Vocabulary

### DLM (Design Language Model)

A Design Language Model captures the compounded sum of fractional design decisions — the infinitesimal judgments designers make that machines don't know. It resolves a brand description into a complete, production-ready expression system. The model that makes taste computable.

### WGLL (What Good Looks Like)

Runtime quality judgment — structured assessment of what good looks like, available to your coding agent at the moment it builds. Not aesthetic opinion. Not subjective review. Encoded taste as a callable runtime check.

### ContentKit

The content intelligence layer — voice-modulated, surface-aware content that speaks in your brand's voice across every touchpoint. Content tokens compiled to match tone, length, and terminology rules.

### EvidenceKit

The quality proof layer — structured evidence that what was built is actually on-brand. Not opinion. Traceable verification with domain-level scoring and pass/fail gates.

### DecisionKit

Pattern resolution — matching what you want to build with structural patterns that carry the brand's judgment. Intent in, governed pattern out.

### Blueprint

A structural pattern matched to brand intent — the starting composition your agent builds from, carrying layout, content zones, and pattern slots.

### Brand Archetype

The personality profile that shapes how a brand expresses itself — the starting point for every design decision the system makes.

### Composition Quality

How well components compose into governed layouts — structural quality that goes beyond visual appearance. The measure of whether the whole is greater than its parts.

### Provenance

Every LESS-generated surface carries provenance — traceable lineage back to the brand decisions that produced it. Not watermarking. Decision archaeology.

### Inner Loop

The inner loop finds and fixes token escapes — hardcoded values that should be brand-governed — and validates the corrections automatically. Continuous quality improvement without manual auditing.

## The Three Design Eras

**Craft** — Design lived in the designer's head. Consistency through proximity.

**Systems** — Design encoded in shared artifacts: style guides, component libraries, tokens. Figma, Storybook, Tokens Studio.

**Post-Static** — Design is infrastructure available at runtime, when the agent builds. The artifact is no longer enough.

[Full definition →](https://designless.live/vocabulary/post-static-design-era)

## Design Language Model (DLM)

The intelligence layer that transforms natural language into structured design decisions. Given a vibe, a keyword, or a description, the DLM resolves it into a complete, coherent set of design tokens — not by guessing, but by understanding how design dimensions relate to each other. It's the reason a single sentence can produce 300+ tokens that feel intentional.

## WGLL (Weighted Generative Layout Language)

The structural vocabulary that encodes how visual elements should be arranged. WGLL captures layout intent — not as pixel coordinates, but as weighted relationships between composition elements. It's how the system knows that a hero section should breathe differently than a dashboard grid, even when both use the same brand tokens.

## ContentKit

The content compilation layer that ensures brand voice extends beyond visual tokens. ContentKit resolves tone, terminology, and messaging patterns into structured content tokens — so when an agent writes a CTA or error message, the words carry the same brand intent as the colors and typography.

## EvidenceKit

The validation engine that proves output is on-brand with traceable evidence chains. EvidenceKit doesn't just check if something looks right — it traces every design decision back to its source token, scores coherence across dimensions, and produces structured proof that can be reviewed, shared, or audited.

## DecisionKit

The reasoning layer that captures why a design decision was made, not just what it is. Every token in the system carries provenance — a traceable chain from user intent through resolution to final value. DecisionKit structures this reasoning so agents can explain their design choices and humans can verify them.

## Blueprint

A structural template that describes how content should be arranged within a composition. Blueprints are matched to user intent — when you say "landing page hero," the system selects a blueprint that encodes the right content zones, emphasis hierarchy, and rhythm for that surface type.

## Brand Archetype

A canonical personality anchor that shapes how a brand expresses itself. Archetypes capture the fundamental character of a brand — whether it's a technical vanguard, a warm community platform, or a premium authority. They influence every design decision from color temperature to typography personality to content tone.

## Composition Quality

A measured score that captures how well visual elements work together in a specific context. Composition quality is not aesthetic opinion — it's structured evaluation across dimensions like token compliance, layout coherence, accessibility, and expression contract adherence. Every generation gets a score, and the score is traceable.

## Provenance

The traceable chain from a user's intent to a generated output. Every token, layout decision, and content choice in the system carries provenance — the record of why it was chosen, what influenced it, and how it relates to the brand's overall expression. Provenance is what makes expression infrastructure auditable.

## Inner Loop

The self-healing cycle that detects when generated output deviates from brand intent and corrects it before delivery. When a token escapes — a hardcoded color, a font override, a spacing anomaly — the inner loop identifies the violation, diagnoses the cause, patches the output, and validates the fix. Continuous quality enforcement, not post-hoc review.
