FROM node:24-bookworm-slim AS web
WORKDIR /build
COPY package*.json ./
RUN npm ci --no-audit --no-fund
COPY tsconfig.json vite.config.ts ./
COPY client ./client
RUN npm run build

FROM rust:1-bookworm AS server
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY server ./server
RUN cargo build --release --locked

FROM debian:bookworm-slim AS runtime
RUN groupadd --gid 10001 wordrelay && useradd --uid 10001 --gid 10001 --no-create-home wordrelay
WORKDIR /app
COPY --from=server /build/target/release/wordrelay ./wordrelay
COPY --from=web /build/dist ./dist
COPY data ./data
USER 10001:10001
EXPOSE 3000
CMD ["./wordrelay"]
