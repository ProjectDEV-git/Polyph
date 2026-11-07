#!/usr/bin/env bash
# Usage check for the orchestrator loop. Prints one line per fact and a final
# DECISION line: CONTINUE, PAUSE (5-hour window at or over 75%) or
# WAIT_UNTIL <epoch> (already paused; the window has not reset yet).
#
# Inputs (environment, all optional):
#   POLYPH_TOKEN_LIMIT   token limit of a 5-hour block; enables a percentage
#                        when ccusage has no earlier block to take a max from
#   RATE_STATUS          rate_limit_info.status from get_session
#                        (allowed, allowed_warning, rejected)
#   RATE_TYPE            rate_limit_info.rateLimitType (five_hour, seven_day)
#   RATE_RESETS_AT       rate_limit_info.resetsAt (epoch seconds)
#   PAUSED_UNTIL         epoch seconds saved when the loop paused
set -u

now=$(date -u +%s)
echo "now: $(date -u -d @"$now" '+%Y-%m-%d %H:%M UTC')"

# 1. Already paused: only check whether the reset time has passed.
if [ -n "${PAUSED_UNTIL:-}" ]; then
  if [ "$now" -lt "$PAUSED_UNTIL" ]; then
    echo "paused until: $(date -u -d @"$PAUSED_UNTIL" '+%H:%M UTC') ($(( (PAUSED_UNTIL - now) / 60 )) min left)"
    echo "DECISION: WAIT_UNTIL $PAUSED_UNTIL"
    exit 0
  fi
  echo "reset time passed"
fi

# 2. ccusage: tokens in the active block and when it ends.
json=$(timeout 120 npx --yes ccusage@latest blocks --active --json ${POLYPH_TOKEN_LIMIT:+--token-limit "$POLYPH_TOKEN_LIMIT"} 2>/dev/null)
read -r tokens end_iso <<<"$(printf '%s' "$json" | node -e '
  let s = ""; process.stdin.on("data", d => s += d).on("end", () => {
    try { const b = JSON.parse(s).blocks.find(b => b.isActive);
          console.log(b ? `${b.totalTokens} ${b.endTime}` : "0 none"); }
    catch { console.log("0 none"); } });')"
pct=""
if [ -n "${POLYPH_TOKEN_LIMIT:-}" ] && [ "$tokens" -gt 0 ] 2>/dev/null; then
  pct=$(( tokens * 100 / POLYPH_TOKEN_LIMIT ))
fi
end_epoch=""
[ "$end_iso" != "none" ] && end_epoch=$(date -u -d "$end_iso" +%s)
echo "block tokens: $tokens"
echo "block resets: ${end_iso} ${end_epoch:+($(( (end_epoch - now) / 60 )) min left)}"
echo "block percent: ${pct:-unknown (no token limit known)}"

# 3. The plan's own signal from get_session.
echo "rate limit: type=${RATE_TYPE:-unknown} status=${RATE_STATUS:-unknown} resets=${RATE_RESETS_AT:-unknown}"

# 4. Decide.
if [ "${RATE_TYPE:-}" = "seven_day" ] && [ "${RATE_STATUS:-}" != "allowed" ] && [ -n "${RATE_STATUS:-}" ]; then
  echo "DECISION: WEEKLY_STOP ${RATE_RESETS_AT:-}"
  exit 0
fi
reset_at=${RATE_RESETS_AT:-$end_epoch}
[ "${RATE_TYPE:-}" != "five_hour" ] && reset_at=$end_epoch
if { [ -n "$pct" ] && [ "$pct" -ge 75 ]; } ||
   { [ "${RATE_TYPE:-}" = "five_hour" ] && [ "${RATE_STATUS:-}" != "allowed" ] && [ -n "${RATE_STATUS:-}" ]; }; then
  echo "DECISION: PAUSE ${reset_at:-}"
  exit 0
fi
echo "DECISION: CONTINUE"
