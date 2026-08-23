#!/usr/bin/env bash
# Renders livekit.yaml / sip.yaml from .env and (re)starts the stack.
# Run on the host from the directory containing this file.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
[ -f .env ] || { echo ".env missing (see .env.example)" >&2; exit 1; }
set -a; # shellcheck disable=SC1091
source ./.env; set +a
for v in LIVEKIT_DOMAIN LIVEKIT_API_KEY LIVEKIT_API_SECRET LIVEKIT_WEBHOOK_URL; do
  [ -n "${!v:-}" ] || { echo "$v is empty in .env" >&2; exit 1; }
done
mkdir -p generated
umask 077
envsubst '$LIVEKIT_DOMAIN $LIVEKIT_API_KEY $LIVEKIT_API_SECRET $LIVEKIT_WEBHOOK_URL' < livekit.yaml.tmpl > generated/livekit.yaml
envsubst '$LIVEKIT_API_KEY $LIVEKIT_API_SECRET' < sip.yaml.tmpl > generated/sip.yaml
sed -i "s/livekit1.tarams.org/$LIVEKIT_DOMAIN/" Caddyfile
docker compose up -d --remove-orphans
docker compose ps
