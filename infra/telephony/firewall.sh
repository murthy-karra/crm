#!/usr/bin/env bash
# ufw rules for the telephony host (docs/specs/SLICE_006.md §11). Idempotent.
set -euo pipefail
sudo ufw --force reset >/dev/null
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow 22/tcp                       # SSH
sudo ufw allow 80/tcp                       # Let's Encrypt HTTP-01
sudo ufw allow 443/tcp                      # Caddy: wss signaling + Twirp API
sudo ufw allow 7881/tcp                     # LiveKit ICE/TCP fallback
sudo ufw allow 50000:60000/udp              # LiveKit WebRTC media
sudo ufw allow 3478/udp                     # TURN/UDP
sudo ufw allow 5349/tcp                     # TURN/TLS
sudo ufw allow 30000:40000/udp              # TURN relay range (LiveKit default)
sudo ufw allow 5060/udp && sudo ufw allow 5060/tcp   # SIP signaling (Telnyx)
sudo ufw allow 10000:20000/udp              # SIP RTP media
sudo ufw --force enable
sudo ufw status numbered
