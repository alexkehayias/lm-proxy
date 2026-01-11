#!/bin/sh
# Optional sanity checks – remove if you don't need them.
: "${HOST:?HOST not set}"
: "${PORT:?PORT not set}"
: "${UPSTREAM:?UPSTREAM not set}"
: "${METRICS_URL:?METRICS_URL not set}"

exec /lm-proxy \
    --host "$HOST" \
    --port "$PORT" \
    --upstream "$UPSTREAM" \
    --metrics-url "$METRICS_URL"
