FROM rust:1.96.0-bookworm@sha256:5e2214abe154fe26e39f64488952e5c991eeed1d6d6da7cc8381ae83927f0cfc

RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        build-essential \
        ca-certificates \
        curl \
        file \
        git \
        libdrm-dev \
        libgbm-dev \
        libglib2.0-dev \
        libx11-dev \
        libxcomposite-dev \
        libxdamage-dev \
        libxext-dev \
        libxfixes-dev \
        libxrandr-dev \
        libxrender-dev \
        libxtst-dev \
        lsb-release \
        perl \
        pkg-config \
        python3 \
        tar \
        unzip \
        xz-utils \
    && rm -rf /var/lib/apt/lists/* \
    && cargo install just --version 1.43.1 --locked
