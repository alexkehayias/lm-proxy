#!/bin/sh
# Optional sanity checks – remove if you don't need them.
: "${HOST:?HOST not set}"
: "${PORT:?PORT not set}"

# Build args array
set -- --host "$HOST" --port "$PORT"

# UPSTREAM can be a single value or space-separated list for multiple upstreams
# Example: UPSTREAM="openai=https://api.openai.com/v1 anthropic=https://api.anthropic.com/v1"
if [ -n "$UPSTREAM" ]; then
    for upstream in $UPSTREAM; do
        set -- "$@" --upstream "$upstream"
    done
fi

# Optional metrics URL
if [ -n "${METRICS_URL:-}" ]; then
    set -- "$@" --metrics-url "$METRICS_URL"
fi

exec /lm-proxy "$@"
