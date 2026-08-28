# Security

## Reporting

Email ai@designless.io with "SECURITY" in the subject for vulnerabilities or any security question. Please don't open public issues for vulnerabilities.

## Supported versions

The latest 1.12.x release. The plugin advises you in-session when an update is available.

## What runs where

This plugin is a thin client. The agent prose, hooks, and a small local bridge run on your machine; every design decision is resolved by the Designless service at mcp.designless.app over TLS. The bridge speaks MCP on stdio locally and never opens a listening port.

## What leaves your machine

Only what a request needs: your prompt and intent, content you ask to have composed, and references to your brands. When you bring your own app onto the canvas, captured pages travel to your workspace, and edits land only on a dedicated containment branch in your checkout. Designless never holds a GitHub token and never pushes to your default branch.

## What never leaves

Credentials. Authentication is held by the Designless desktop app (or a browser OAuth flow); this plugin's files contain no keys, and the bridge never logs or returns tokens.

## Tamper evidence

Every release ships a signed bridge binary, and the service attests the plugin's files on connection. A modified install is refused, with a recovery hint, rather than served.

## Third-party code

The compiled bridge statically links open-source crates inventoried in [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md), regenerated on every dependency change.
