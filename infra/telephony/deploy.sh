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
envsubst '$LIVEKIT_API_KEY $LIVEKIT_API_SECRET' < egress.yaml.tmpl > generated/egress.yaml
if [ -n "${EGRESS_S3_BUCKET:-}" ]; then
  : "${AWS_REGION:?AWS_REGION must be set when EGRESS_S3_BUCKET is set}"
  envsubst '$EGRESS_S3_BUCKET $AWS_REGION' < egress-s3.yaml.tmpl >> generated/egress.yaml
fi
# The egress image runs as a non-root user, so its config cannot be 0600 like
# the others; keep the directory private instead (the bind mount resolves the
# file by inode, other host users still cannot traverse generated/).
chmod 700 generated
chmod 644 generated/egress.yaml
# Site address line is "<host> {" — replace whatever host is there, so a
# domain change re-renders cleanly instead of assuming the checked-in name.
sed -i -E "s/^[A-Za-z0-9.-]+ \{$/$LIVEKIT_DOMAIN {/" Caddyfile
docker compose up -d --remove-orphans
docker compose ps
