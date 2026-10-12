# The CaveCAD server: the web app plus the live-editing relay, with room logs kept on a volume.
# Build from the repo root:   docker build -f apps/server/Dockerfile -t cavecad-server .
# (or use apps/server/docker-compose.yml)

# ---- build the web app (Rust engine -> WASM -> Vite) ------------------------------------------
FROM node:22-bookworm-slim AS build
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl build-essential pkg-config libssl-dev git \
 && rm -rf /var/lib/apt/lists/*
ENV RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo PATH=/root/.cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal -t wasm32-unknown-unknown \
 && cargo install wasm-pack --locked

WORKDIR /src
# dependencies first, so editing code does not reinstall them
COPY apps/web/package.json apps/web/package-lock.json apps/web/
RUN cd apps/web && npm ci --no-audit --no-fund
COPY . .
# the Vite plugin runs wasm-pack (release) as part of the build
RUN cd apps/web && npm run build

# ---- the small runtime image ---------------------------------------------------------------------
FROM node:22-bookworm-slim
WORKDIR /app
COPY apps/server/package.json apps/server/package-lock.json* ./
RUN npm install --omit=dev --no-audit --no-fund
COPY apps/server/server.mjs apps/server/room-store.mjs server/
COPY apps/web/scripts/sync-relay.mjs web/scripts/
COPY --from=build /src/apps/web/dist web/dist

ENV PORT=8080 DATA_DIR=/data
RUN mkdir /data && chown node:node /data
VOLUME /data
USER node
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD node -e "fetch('http://127.0.0.1:'+process.env.PORT+'/healthz').then(r=>process.exit(r.ok?0:1),()=>process.exit(1))"
CMD ["node", "server/server.mjs"]
