# Version Protocol

Designless agent follows `1.x.y` versioning starting from `1.0.0`.

## Version Format: `1.x.y`

- **1** — Major version. Stays at 1 until a breaking change to the plugin contract (mode structure, MCP server protocol, or command interface).
- **x** — Enhancement release. New modes, new commands, new sub-agent integrations, significant capability additions.
- **y** — Minor bump. Bug fixes, copy improvements, behavioral refinements, manifest updates.

## When to Bump

| Change | Bump | Example |
|--------|------|---------|
| Fix typo in SKILL.md | y | 1.0.0 → 1.0.1 |
| Improve mode classification logic | y | 1.0.1 → 1.0.2 |
| Add new command (e.g., `/designless:compare`) | x | 1.0.2 → 1.1.0 |
| Integrate new sub-agent | x | 1.1.0 → 1.2.0 |
| Add offline capsule support | x | 1.2.0 → 1.3.0 |
| Change mode classification structure | Major (2.0.0) | Only if modes fundamentally restructure |

## Where to Update

When bumping version, update these files:
1. `.claude-plugin/plugin.json` — `version` field
2. `.claude-plugin/marketplace.json` — `version` field in plugins array

Both must match. Commit message format: `v1.x.y: description of change`

## Release Tags

Tag releases on GitHub: `git tag -a v1.x.y -m "description"` then `git push origin v1.x.y`.

Enhancement releases (x bumps) get GitHub release pages. Minor bumps (y) are commit-only.
