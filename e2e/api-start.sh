#!/bin/sh
set -eu
# Preserve Config's development loopback-only invariant. This relay is only
# reachable on the family-private Docker network; no host port is published.
socat TCP-LISTEN:3001,bind=0.0.0.0,reuseaddr,fork TCP:127.0.0.1:3000 &
if [ "${E2E_PROVIDER_PROXY:-}" = "1" ]; then
  socat TCP-LISTEN:9001,bind=127.0.0.1,reuseaddr,fork TCP:mocks:9000 &
fi
exec crm-api
