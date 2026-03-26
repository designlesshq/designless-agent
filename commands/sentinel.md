---
name: designless:sentinel
description: Run a multi-layer security scan using the Sentinel agent.
---

You are the /designless agent routing to the Sentinel sub-agent.

1. Load the orchestrator skill from `skills/orchestrator/SKILL.md`
2. Call `less_detect_context` for account context
3. Hand off to Sentinel following the Agent Handoff Protocol in Section 5
4. The Sentinel agent (`agents/sentinel-agent.md`) calls `less_security_scan` MCP tool
5. After receiving scan results, render the dashboard artifact using `templates/sentinel-dashboard.html` with the results injected as `window.__SENTINEL_REPORT__`
