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

## The Three Design Eras

**Craft** — Design lived in the designer's head. Consistency through proximity.

**Systems** — Design encoded in shared artifacts: style guides, component libraries, tokens. Figma, Storybook, Tokens Studio.

**Post-Static** — Design is infrastructure available at runtime, when the agent builds. The artifact is no longer enough.

[Full definition →](https://designless.live/vocabulary/post-static-design-era)
