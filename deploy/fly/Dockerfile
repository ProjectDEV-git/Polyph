# The relay for Fly.io, built from source (see fly.toml). Build context: the
# repository root. Unlike deploy/relay/Dockerfile this runs as root: Fly
# mounts the volume at /data owned by root, and the app already runs alone in
# its own VM.
FROM rust:1.97-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --profile dist --locked -p polyph-relay \
 && cp target/dist/polyph-relay /polyph-relay

FROM gcr.io/distroless/cc-debian12
COPY --from=build /polyph-relay /usr/local/bin/polyph-relay
ENV POLYPH_RELAY_BIND=0.0.0.0:8787 \
    POLYPH_RELAY_DATABASE=/data/relay.db
EXPOSE 8787
ENTRYPOINT ["/usr/local/bin/polyph-relay"]
