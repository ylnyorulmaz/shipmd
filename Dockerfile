# Ship Board: one image with the Rust backend serving the built SolidJS frontend.

# ---- 1. Frontend -------------------------------------------------------------
FROM node:22-bookworm-slim AS web
WORKDIR /web
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# ---- 2. Backend --------------------------------------------------------------
FROM rust:1-bookworm AS api
WORKDIR /src
# Build dependencies first so code-only changes reuse this layer.
COPY backend/Cargo.toml backend/Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
 && cargo build --release --locked \
 && rm -rf src
COPY backend/src ./src
RUN touch src/main.rs && cargo build --release --locked

# ---- 3. Runtime --------------------------------------------------------------
# TLS roots for Telegram are compiled into the binary (rustls + webpki-roots), so no apt step is needed.
FROM debian:bookworm-slim
WORKDIR /app
COPY --from=api /src/target/release/shipmd-backend /app/shipmd-backend
COPY --from=web /web/dist /app/static

ENV PORT=8080 \
    BOARD_DB=/data/board.db \
    STATIC_DIR=/app/static \
    RUST_LOG=info,sqlx=warn
EXPOSE 8080
VOLUME ["/data"]
CMD ["/app/shipmd-backend"]
