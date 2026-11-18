# syntax=docker/dockerfile:1
# Cross-compile on the build host instead of emulating each target with QEMU.
FROM --platform=$BUILDPLATFORM rust:1.99.1-alpine AS build
ARG TARGETARCH
WORKDIR /src
COPY rust-toolchain.toml ./
RUN rustup toolchain install
# rust-lld links the musl targets' self-contained static libc for any host architecture.
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    case "$TARGETARCH" in \
        arm64) target=aarch64-unknown-linux-musl ;; \
        amd64) target=x86_64-unknown-linux-musl ;; \
        *) echo "unsupported architecture $TARGETARCH" >&2; exit 1 ;; \
    esac \
    && cargo build --release --locked --target "$target" \
    && cp "target/$target/release/mdns-tunnel" /mdns-tunnel

# docker build --target export --output dist . writes the binary to the host.
FROM scratch AS export
COPY --from=build /mdns-tunnel /

FROM scratch
COPY --from=build /mdns-tunnel /mdns-tunnel
ENTRYPOINT ["/mdns-tunnel"]
