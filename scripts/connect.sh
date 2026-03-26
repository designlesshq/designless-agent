#!/bin/bash
# Designless MCP Connection Script
# Opens the OAuth authorization URL in the default browser
# and waits for the user to complete the flow.

LESS_MCP_URL="${LESS_MCP_URL:-https://mcp.designless.studio}"
echo "Opening Designless authorization..."
echo "After authorizing, copy your API key from the dashboard."
echo ""
echo "1. Visit: ${LESS_MCP_URL/mcp/}"
echo "2. Sign up or log in"
echo "3. Go to API Keys and create a key"
echo "4. Add to your environment:"
echo "   export LESS_API_KEY=your_key_here"
echo ""
echo "Then add the MCP server:"
echo '   claude mcp add --transport http designless \'
echo '     --header "x-api-key: $LESS_API_KEY" \'
echo "     ${LESS_MCP_URL}/mcp"
