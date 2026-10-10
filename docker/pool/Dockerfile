# The pool's own services. Build from the repository root with one of the targets:
#
#   docker build -f docker/pool/Dockerfile --target stratum .
#   docker build -f docker/pool/Dockerfile --target core .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY migrations migrations
RUN cargo build --release --locked -p pool-stratum -p pool-core

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --create-home --uid 1000 pool
WORKDIR /app
COPY config/production.toml config/production.toml
ENV POOL_CONFIG=/app/config/production.toml
USER pool

# One per region. Publish the stratum ports (see config/production.toml).
FROM runtime AS stratum
COPY --from=build /src/target/release/pool-stratum /usr/local/bin/pool-stratum
EXPOSE 3333 3334
CMD ["pool-stratum"]

# Exactly one in the whole pool: it does the accounting and sends the payouts.
FROM runtime AS core
COPY --from=build /src/target/release/pool-core /usr/local/bin/pool-core
CMD ["pool-core"]
