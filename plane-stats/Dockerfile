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
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
 && curl -fsSL -o /aircraftDatabase.csv \
      https://opensky-network.org/datasets/metadata/aircraftDatabase.csv \
 && rm -rf /var/lib/apt/lists/*
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /build/target/release/plane-stats /usr/local/bin/plane-stats
COPY --from=build /aircraftDatabase.csv /usr/share/plane-stats/aircraftDatabase.csv
ENV DB_PATH=/data/planes.db
ENV ENRICHMENT_CSV=/usr/share/plane-stats/aircraftDatabase.csv
EXPOSE 8081
CMD ["plane-stats"]
