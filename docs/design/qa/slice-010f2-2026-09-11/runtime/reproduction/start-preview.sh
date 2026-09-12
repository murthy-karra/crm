#!/bin/zsh
set -eu
test "${QA_RUNTIME_AUTHORIZED:-}" = yes || { print -u2 'Root must authorize runtime after R2 and gates.'; exit 1; }
cd /Users/karrad/projects/crm-010f2/web
export PATH=/Users/karrad/.nvm/versions/node/v24.16.0/bin:$PATH
export CRM_WEB_BIND_ADDR=127.0.0.1 CRM_WEB_PORT=5187
export CRM_WEB_API_PROXY_TARGET=http://127.0.0.1:3017
export CRM_WEB_REALTIME_PROXY_TARGET=http://127.0.0.1:18082
exec /Users/karrad/Library/pnpm/store/v11/links/@/pnpm/11.22.0/eeb737e15b4ed7190c895e85812ddc0832a617564aa8721c8139de5d87d3a2b4/bin/pnpm exec vite preview --host 127.0.0.1 --port 5187 --strictPort
