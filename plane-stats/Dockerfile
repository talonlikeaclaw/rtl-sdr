# CSV downloads in their own stage: cached independently of source changes, so
# rebuilds don't re-download ~94 MB. Bump CSV_VERSION to force a fresh fetch of
# either file.
FROM rust:1-slim AS csv
ARG CSV_VERSION=1
RUN echo "csv-version=$CSV_VERSION" \
 && apt-get update && apt-get install -y --no-install-recommends curl ca-certificates gzip \
 && curl -fsSL -o /aircraftDatabase.csv \
      https://opensky-network.org/datasets/metadata/aircraftDatabase.csv \
 && curl -fsSL -o /aircraft-db.csv.gz \
      https://raw.githubusercontent.com/wiedehopf/tar1090-db/csv/aircraft.csv.gz \
 && gzip -dc /aircraft-db.csv.gz > /aircraft-db.csv \
 && rm -rf /var/lib/apt/lists/*

FROM rust:1-slim AS build
WORKDIR /build
# Cache deps separately so code changes don't re-fetch them.
COPY Cargo.toml .
COPY Cargo.lock .
RUN mkdir src && echo "fn main() {}" > src/main.rs \
 && cargo build --release 2>/dev/null || true \
 && rm -rf src
COPY src src
COPY templates templates
COPY static static
RUN touch src/main.rs && cargo build --release
COPY --from=csv /aircraftDatabase.csv /aircraftDatabase.csv
COPY --from=csv /aircraft-db.csv /aircraft-db.csv

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /build/target/release/plane-stats /usr/local/bin/plane-stats
COPY --from=build /aircraftDatabase.csv /usr/share/plane-stats/aircraftDatabase.csv
COPY --from=build /aircraft-db.csv /usr/share/plane-stats/aircraft-db.csv
ENV DB_PATH=/data/planes.db
ENV ENRICHMENT_CSV=/usr/share/plane-stats/aircraftDatabase.csv
ENV TAR1090_DB_CSV=/usr/share/plane-stats/aircraft-db.csv
EXPOSE 8081
CMD ["plane-stats"]
