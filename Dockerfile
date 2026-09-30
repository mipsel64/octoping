FROM rust:1-slim-trixie AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked && cp target/release/octoping /octoping

# Same Debian release as the builder (glibc must match); ships CA certs for Discord TLS.
FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=build /octoping /octoping
EXPOSE 8080
ENTRYPOINT ["/octoping"]
CMD ["--config", "/etc/octoping/octoping.yaml"]
