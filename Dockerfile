#### Multi-stage build for kkss-backend using cargo-chef (workspace: root + migration)

# 0) Base chef image with cargo-chef available
FROM rust:1.91 AS chef
WORKDIR /build
RUN cargo install cargo-chef

# 1) Planner stage: generate recipe capturing workspace dependency graph
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# 2) Builder stage: install system deps, cook dependencies from recipe, then build
FROM chef AS builder
ARG APP_NAME=kkss-backend
WORKDIR /build

# Cook cached dependencies for the entire workspace (root crate + migration)
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Build the actual application (this layer rebuilds only when source changes)
COPY . .
RUN cargo build --release --locked --bin ${APP_NAME}

## Runtime stage
FROM debian:bookworm-slim AS runtime

WORKDIR /app

# Copy binary
COPY --from=builder /build/target/release/kkss-backend ./kkss-backend
COPY --from=builder /build/migration ./migration

# Copy entrypoint
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

# Create non-root user
RUN useradd -u 10001 -ms /bin/bash appuser && chown -R appuser:appuser /app
USER appuser

# Expose typical Actix port (adjust if your config.toml uses another)
EXPOSE 8080

# Working directory will have config.toml & kkss.db mounted at runtime:
#   docker run -v $(pwd)/config.toml:/app/config.toml -v $(pwd)/kkss.db:/app/kkss.db IMAGE

ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["./kkss-backend"]
