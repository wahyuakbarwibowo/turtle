# Build context: repo root (needs the workspace, crates/ and migrations/).
FROM rust:1-slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p server && cp target/release/server /server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 turtlemeet
COPY --from=build /server /usr/local/bin/server
USER turtlemeet
EXPOSE 8080
# Migrations are embedded and run on start. CLI: `server integration create <name>`.
CMD ["server"]
