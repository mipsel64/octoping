FROM rust:1-slim-trixie AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked && cp target/release/octoping /octoping

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=build /octoping /usr/bin/octoping
EXPOSE 8080
ENTRYPOINT ["/usr/bin/octoping"]
CMD ["--config", "/etc/octoping/octoping.yaml"]
