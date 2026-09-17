# syntax=docker/dockerfile:1
FROM rust:1.98.0-slim-bookworm AS api-build
WORKDIR /build/backend
ENV SQLX_OFFLINE=true CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 CARGO_PROFILE_RELEASE_OPT_LEVEL=1
COPY backend/ ./
RUN cargo build --locked --release -p crm-api --bin crm-api --bin migrate --bin crm-admin

FROM debian:bookworm-slim AS api
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl socat && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=api-build /build/backend/target/release/crm-api /build/backend/target/release/migrate /build/backend/target/release/crm-admin /usr/local/bin/
COPY --chmod=755 e2e/api-start.sh /usr/local/bin/e2e-api
CMD ["e2e-api"]

FROM node:26.8.2-bookworm-slim AS web-build
WORKDIR /build/web
RUN npm install --global pnpm@11.22.0
COPY web/package.json web/pnpm-lock.yaml web/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm exec vite build --outDir /build/output

FROM nginx:1.28.0-alpine AS web
COPY --from=web-build /build/output /usr/share/nginx/html
COPY e2e/nginx.conf /etc/nginx/conf.d/default.conf

FROM mcr.microsoft.com/playwright:v1.63.0-noble AS browser
WORKDIR /e2e
COPY e2e/package.json e2e/package-lock.json ./
RUN npm ci --ignore-scripts
COPY e2e/playwright.config.mjs ./
COPY e2e/families/ ./families/
COPY e2e/support/ ./support/
CMD ["npx", "--no-install", "playwright", "test"]


# Migration uses the existing injected Reader seam. All HTTP/session/domain/
# worker/realtime/database implementations are real; live FUB HTTP is not proven.
FROM api-build AS migration-build
COPY e2e/migration-server.rs ./crates/crm-api/examples/e2e_migration_server.rs
RUN cargo build --locked --release -p crm-api --features test-support --example e2e_migration_server
FROM api AS migration-api
COPY --from=migration-build /build/backend/target/release/examples/e2e_migration_server /usr/local/bin/
CMD ["e2e_migration_server"]

# Only the external LiveKit SDK boundary differs from the normal production Web.
FROM web-build AS calls-web-build
COPY e2e/calls-vite.config.mjs ./calls-vite.config.mjs
COPY e2e/livekit-browser.mjs ./e2e-livekit-browser.mjs
RUN pnpm exec vite build --config calls-vite.config.mjs --outDir /build/calls-output
FROM nginx:1.28.0-alpine AS calls-web
COPY --from=calls-web-build /build/calls-output /usr/share/nginx/html
COPY e2e/nginx.conf /etc/nginx/conf.d/default.conf
