ARG NGX_VERSION=1.28.0
# NGINX official images are available for one specific release of Debian,
# e.g `nginx:1.28.0-bookworm`. Please see the list of tags on docker hub
# if you need to change NGX_VERSION.
ARG DEBIAN_RELEASE=bookworm

# --- builder: build all examples
FROM rust:slim-${DEBIAN_RELEASE} AS build
ARG NGX_VERSION
ARG NGX_CONFIGURE_ARGS=
WORKDIR /project
RUN --mount=type=cache,target=/var/cache/apt <<EOF
    set -eux
    export DEBIAN_FRONTEND=noninteractive
    apt-get -qq update
    apt-get -qq install --yes --no-install-recommends --no-install-suggests \
        libclang-dev \
        libpcre2-dev \
        libssl-dev \
        zlib1g-dev \
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
FROM nginx:${NGX_VERSION}-${DEBIAN_RELEASE}

RUN mkdir -p /etc/nginx/examples

COPY --from=build /out/*.so /etc/nginx/modules/
COPY --from=build /project/examples/*.conf /etc/nginx/examples

EXPOSE 8000
