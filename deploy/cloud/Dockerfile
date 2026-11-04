# Polyph's hosted service (proprietary, cloud/): the account API and the
# hosted relay with plan limits. Build from the repository root:
#   docker build -f deploy/cloud/Dockerfile -t polyph-cloud .
FROM rust:1.97-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY locales ./locales
COPY cloud ./cloud
RUN cargo build --manifest-path cloud/Cargo.toml --release --locked \
 && cp cloud/target/release/polyph-cloud /polyph-cloud

# An empty /data owned by the image's non-root user (65532); named volumes
# start with this ownership.
FROM busybox:1.37 AS data
RUN mkdir -p /data && chown 65532:65532 /data

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=data --chown=65532:65532 /data /data
COPY --from=build /polyph-cloud /usr/local/bin/polyph-cloud
ENV POLYPH_CLOUD_CONFIG=/etc/polyph/cloud.toml
VOLUME /data
EXPOSE 8787 8790
USER nonroot
ENTRYPOINT ["/usr/local/bin/polyph-cloud", "serve"]
