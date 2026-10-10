FROM debian:bookworm-slim

ARG VERSION=0.21.5.8
# The digest GitHub publishes for the release asset; it is also in the signed SHA256SUMS.asc.
ARG SHA256=43200c9f9d65ebc126ea5833ca9429e144c4b3273da6bb9f4e89fd7450ab1be9

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && curl -fsSL -o /tmp/litecoin.tar.gz \
      "https://github.com/litecoin-project/litecoin/releases/download/v${VERSION}/litecoin-${VERSION}-x86_64-linux-gnu.tar.gz" \
 && echo "${SHA256}  /tmp/litecoin.tar.gz" | sha256sum -c - \
 && mkdir /opt/litecoin && tar -xzf /tmp/litecoin.tar.gz -C /opt/litecoin \
 && for tool in litecoind litecoin-cli litecoin-wallet; do ln -s "$(find /opt/litecoin -type f -name "$tool")" /usr/local/bin/; done \
 && rm /tmp/litecoin.tar.gz \
 && apt-get purge -y curl && apt-get autoremove -y && rm -rf /var/lib/apt/lists/* \
 && useradd --create-home --uid 1000 coin \
 && mkdir /data && chown coin:coin /data \
 && litecoind -version | head -1

COPY --chmod=755 entrypoint.sh /usr/local/bin/pool-entrypoint
USER coin
VOLUME /data
ENTRYPOINT ["pool-entrypoint"]
