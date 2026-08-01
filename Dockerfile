# syntax=docker/dockerfile:1.7

FROM rust:1-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libclang-dev \
        libgtk-3-dev \
        libssl-dev \
        libxcb-render0-dev \
        libxcb-shape0-dev \
        libxcb-xfixes0-dev \
        libxkbcommon-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build \
    --locked \
    --release \
    --bin opsdeck

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        git \
        libssl3 \
        openssh-client \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder \
    /build/target/release/opsdeck \
    /usr/local/bin/opsdeck

COPY docker/entrypoint.sh \
    /usr/local/bin/opsdeck-entrypoint

RUN chmod +x \
    /usr/local/bin/opsdeck \
    /usr/local/bin/opsdeck-entrypoint

ENV HOME=/data
ENV OPSDECK_CONTAINER=1

RUN mkdir -p \
    /data/.opsdeck \
    /workspace

WORKDIR /workspace

ENTRYPOINT ["/usr/local/bin/opsdeck-entrypoint"]

CMD ["--help"]