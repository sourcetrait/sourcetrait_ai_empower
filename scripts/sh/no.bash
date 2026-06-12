#!/usr/bin/env bash
cat > /dev/null   # drain stdin so Claude Code doesn't see a broken pipe
cat <<'EOF'
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "Do not use Bash. Use the Nushell MCP instead."
  }
}
EOF
exit 0

