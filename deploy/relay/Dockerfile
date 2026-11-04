# The Polyph relay as a small container. Build from the repository root:
#   docker build -f deploy/relay/Dockerfile -t polyph-relay .
FROM rust:1.97-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --profile dist --locked -p polyph-relay \
 && cp target/dist/polyph-relay /polyph-relay

# An empty /data owned by the image's non-root user (65532); named volumes
# start with this ownership.
FROM busybox:1.37 AS data
RUN mkdir -p /data && chown 65532:65532 /data

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=data --chown=65532:65532 /data /data
COPY --from=build /polyph-relay /usr/local/bin/polyph-relay
ENV POLYPH_RELAY_BIND=0.0.0.0:8787 \
    POLYPH_RELAY_DATABASE=/data/relay.db
VOLUME /data
EXPOSE 8787
USER nonroot
ENTRYPOINT ["/usr/local/bin/polyph-relay"]
