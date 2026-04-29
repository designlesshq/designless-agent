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

When bumping version, update these **four** locations. All must match for the in-product update nudge (see brain row [`59db8687`](https://supabase.com/dashboard/project/mjvbhqaqiqfgxgzicrao)) to work — drift means users either get a false-positive nudge or never hear about the new version.

1. `.claude-plugin/plugin.json` — `version` field
2. `.claude-plugin/marketplace.json` — `version` field in the plugins array
3. `skills/orchestrator/SKILL.md` — `plugin_version` in the frontmatter **and** the version literal in the "Plugin self-update" section body. The agent reads the frontmatter to know its own installed version and the section body documents how it nudges. Both must agree.
4. **Server-side** in the `designsystem` repo: `supabase/functions/less-mcp/server.ts` constant `LATEST_PLUGIN_VERSION`. This is what the orchestrator compares its installed version *against* — bump server-side AFTER the new plugin tag is published, so users on the new tag don't see a stale nudge against themselves.

Commit message format: `v1.x.y: description of change`

### Bump-order playbook (do not interleave)

```
# 1. In designless-agent — bump all 3 locations
edit plugin.json + marketplace.json + skills/orchestrator/SKILL.md
git commit -m "v1.x.y: ..." && git push
git tag v1.x.y && git push origin v1.x.y

# 2. After the tag is live in the marketplace, bump server constant
cd ../designsystem
edit supabase/functions/less-mcp/server.ts (LATEST_PLUGIN_VERSION)
git commit -m "feat(less-mcp): bump LATEST_PLUGIN_VERSION to 1.x.y" && git push
```

Server bump SECOND — if you bump server first, every user on the new (just-published) tag sees a "newer version available" nudge that points at themselves until they restart.

## Release Tags

Tag releases on GitHub: `git tag -a v1.x.y -m "description"` then `git push origin v1.x.y`.

Enhancement releases (x bumps) get GitHub release pages. Minor bumps (y) are commit-only.
