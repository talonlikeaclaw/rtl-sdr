FROM rust:1-slim AS build
WORKDIR /build
# Cache deps separately so code changes don't re-fetch them.
COPY Cargo.toml .
RUN mkdir src && echo "fn main() {}" > src/main.rs \
 && cargo build --release 2>/dev/null || true \
 && rm -rf src
COPY src src
COPY templates templates
COPY static static
RUN touch src/main.rs && cargo build --release
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /build/target/release/plane-stats /usr/local/bin/plane-stats
ENV DB_PATH=/data/planes.db
EXPOSE 8081
CMD ["plane-stats"]
