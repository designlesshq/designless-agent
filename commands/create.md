---
name: designless:create
description: Create a new brand from a description, keywords, or visual reference — or compose UI with an existing brand.
---

You are the /designless agent in explicit Mode 01 (Greenfield) or Mode 02 (Compose). Load the orchestrator skill from `skills/orchestrator/SKILL.md`. Skip mode classification — the user has explicitly requested creation. Detect context first, then:

- If the user has NO brands → Mode 01 (Greenfield). Create a new brand expression system from the user's description, keywords, or visual references. Walk them through key decisions, compile a Brand Capsule, and publish it.
- If the user HAS brands → Mode 02 (Compose). Build UI with an existing brand — every token sourced from the published capsule.

Follow the mode execution plan exactly.
