# Experimental Apple Silicon build host. Building this image installs software;
# follow the explicit approval requirement in makepad/AGENTS.md first.
FROM ubuntu@sha256:496754492fb28b4d3049432f2ca787449331e23fb14f0dd3fffea86bf5a93eb4

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ca-certificates git git-lfs gnupg flex bison build-essential bc \
        zip curl zlib1g-dev libc6-dev-i386 libx11-dev lib32z1-dev \
        libgl1-mesa-dev libxml2-utils xsltproc unzip fontconfig python3 \
        repo rsync openssl lz4 libelf-dev \
    && mkdir -p /opt/octosense \
    && dpkg-query -W > /opt/octosense/packages.tsv \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
CMD ["/bin/bash"]
