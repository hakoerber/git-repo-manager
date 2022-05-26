FROM docker.io/debian:11.3

RUN apt-get update \
    && apt-get install -y \
        python3-pytest \
        python3-toml \
        python3-git \
        python3-yaml \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /tests

ENTRYPOINT ["/bin/sh", "-c", "--"]
