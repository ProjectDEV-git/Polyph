#!/usr/bin/env bash
# Status line that saves Claude Code's status JSON (including rate_limits)
# for usage-check.sh, then prints a short line.
f="${CLAUDE_USAGE_FILE:-$HOME/.claude/usage-status.json}"
cat > "$f.tmp" && mv "$f.tmp" "$f"
node -e 'const s=require(process.argv[1]);const r=(s.rate_limits||{}).five_hour;console.log(r?`5h ${Math.round(r.used_percentage)}%`:"5h ?")' "$f" 2>/dev/null || echo "5h ?"
