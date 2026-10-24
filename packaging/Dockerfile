# glibc 2.36 keeps the binary usable on Debian 12 and newer distributions.
FROM rust:1.99.0-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 pkg-config libxkbcommon-dev desktop-file-utils \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install --locked --version 0.11.8 cargo-packager \
    && cargo install --locked --version 0.21.0 cargo-generate-rpm
WORKDIR /work
ENV XDG_CACHE_HOME=/tmp/sysorbit-cache
CMD ["bash", "packaging/build.sh"]
