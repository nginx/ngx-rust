ARG NGX_VERSION=1.28.0
ARG NGX_DEBUG=false

# --- builder: build all examples
FROM rust:slim-bullseye AS build
ARG NGX_VERSION
ARG NGX_DEBUG
WORKDIR /project
RUN --mount=type=cache,target=/var/cache/apt <<EOF
    set -eux
    export DEBIAN_FRONTEND=noninteractive
    apt-get -qq update
    apt-get -qq install --yes --no-install-recommends --no-install-suggests \
        libclang-dev \
        libssl-dev \
        pkg-config \
        git \
        grep \
        gawk \
        gnupg2 \
        sed \
        make
    git config --global --add safe.directory /project
EOF

COPY . .

RUN --mount=type=cache,id=cargo,target=/usr/local/cargo/registry \
    cargo fetch --locked

RUN --mount=type=cache,id=target,target=target \
    --mount=type=cache,id=cache,target=.cache \
    --mount=type=cache,id=cargo,target=/usr/local/cargo/registry \
    mkdir -p /out && \
    cargo build --release --package examples --examples && \
    mv /project/target/release/examples/*.so /out

# --- copy dynamic modules into official nginx image from builderclear
FROM nginx:${NGX_VERSION}
ARG NGX_VERSION

RUN mkdir -p /etc/nginx/examples

COPY --from=build /out/*.so /etc/nginx/modules/
COPY --from=build /project/examples/*.conf /etc/nginx/examples

EXPOSE 8000
