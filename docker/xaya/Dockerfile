# The official xaya/xaya Docker image is built without wallet support, so this
# uses the release binaries instead.
FROM debian:bookworm-slim

ARG VERSION=1.7.0.1
# The project publishes no checksums; this is the hash of the file as first downloaded (2026-10-10).
ARG SHA256=9477c02ef9230c080a8630c2f7d5824df466817ed2ac446f830c67ebf6b34efa

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl unzip \
 && curl -fsSL -o /tmp/xaya.zip \
      "https://github.com/xaya/xaya/releases/download/v${VERSION}/xaya-1.7-linux-binaries.zip" \
 && echo "${SHA256}  /tmp/xaya.zip" | sha256sum -c - \
 && unzip -j /tmp/xaya.zip xayad xaya-cli -d /usr/local/bin \
 && chmod +x /usr/local/bin/xayad /usr/local/bin/xaya-cli \
 && rm /tmp/xaya.zip \
 && apt-get purge -y curl unzip && apt-get autoremove -y && rm -rf /var/lib/apt/lists/* \
 && useradd --create-home --uid 1000 coin \
 && mkdir /data && chown coin:coin /data

USER coin
VOLUME /data
ENTRYPOINT ["xayad", "-datadir=/data", "-printtoconsole"]
