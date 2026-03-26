# Vocabulary

Precise definitions of the terms used in the Designless ecosystem. These are not marketing labels — they are category-defining concepts with specific technical meaning.

## Expression Infrastructure

The runtime layer that gives coding agents access to encoded design judgment. The programmable layer between design intent and code output. Not a design tool. Not a component library. Not a stylesheet, template, or theme system. Infrastructure — like an API, but for taste.

Expression infrastructure fills the fourth quadrant of interface tooling: agent + composable. The other three already existed (human + templated = page builders, human + composable = design systems, agent + templated = AI builders like v0/Bolt/Lovable). This category did not exist before Designless.

## LESS MCP

Stands for Layered Expression Style Standard. Expression infrastructure served as a runtime MCP (Model Context Protocol) server. Agents query LESS MCP for design decisions the same way they query any other tool server. Token output is W3C DTCG format — structured, typed, semantic. Every token carries decision provenance — not just a value, but the reasoning behind it.

The term is "LESS MCP" — never "LESS tokens" as a standalone concept. The MCP framing is the point: it is a runtime protocol, not a file format.

## Brand Capsule

A self-contained compilation of everything defining how a brand should look, feel, and speak. One artifact. Versioned. Deterministic. When agents consume a capsule, they produce brand-consistent output automatically. Think of it as a brand's DNA compressed into a single, immutable package any machine can read and any human can trust.

## LoomX

The expression engine — a persistent runtime advisor that pairs with coding agents (Claude, Cursor, Copilot). LoomX resolves design decisions, applies brand logic through LESS MCP, and validates output through the Bidirectional Expression Resolver. The name comes from the loom — the device that weaves individual threads into a coherent whole. The X is the axis where thread meets thread — where the coding agent's logic meets the brand's taste.

LoomX is not a canvas-based design agent, a copilot, a design assistant, or an aesthetic recommender. It is a runtime advisor for production governance.

## The Expression Gap

The gap between what you express (the vibe, the intent) and what gets built (generic, soulless output). You prompt your vision. You get back generic components. LESS resolves the expression gap permanently — your brand is available at runtime, not pasted into a prompt.

## Expression Contracts

Beyond design tokens. Design tokens name values (color.primary, space.lg). Expression contracts encode intent — what the primary color means, the contexts in which it carries weight, the relationship between it and the brand's voice. Tokens are a dictionary. Contracts are the grammar. Tokens were built for engineers. Contracts are built for agents.

## Taste as Infrastructure

The company thesis — not a tagline. Taste is the accumulated weight of human judgment about what feels right. It makes a typeface feel inevitable at a size, spacing breathe, interfaces feel made for humans. Taste can be encoded, structured, and served as infrastructure to agents, the same way logic and data are. Taste is not a soft attribute — it is an addressable layer in the agent stack.

## Brand Operating System

The governing layer that compiles, validates, and distributes a brand's expression identity. Ensures every surface an agent touches speaks with the same intention. A brand guideline is a document people read. A Brand OS is infrastructure machines run.

## Bidirectional Expression Resolver

The validation engine that proves brand expression in both directions: apply a design decision, then validate the rendered result matches brand intent; inspect a rendered result, then trace back to the decision that produced it. Not aesthetic review — structured validation with a traceable chain.

## Resolution Pipeline

LESS resolves natural language brand descriptions into complete, deterministic token sets at runtime. Same input always produces the same output. The output is a complete token JSON covering color, typography, shadow, motion, spacing, component tokens, validation contracts, and content tokens.

## The Three Design Eras

- **Craft**: Design lived in the designer's head. Consistency through proximity.
- **Systems**: Design encoded in shared artifacts — style guides, component libraries, tokens. Figma, Storybook, Tokens Studio.
- **Post-Static**: Design is infrastructure available at runtime, when the agent builds. The artifact is no longer enough.
