FROM debian:bookworm-slim

ARG VERSION=1.14.9
# From the release's SHA256SUMS.asc
ARG SHA256=4f227117b411a7c98622c970986e27bcfc3f547a72bef65e7d9e82989175d4f8

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && curl -fsSL -o /tmp/dogecoin.tar.gz \
      "https://github.com/dogecoin/dogecoin/releases/download/v${VERSION}/dogecoin-${VERSION}-x86_64-linux-gnu.tar.gz" \
 && echo "${SHA256}  /tmp/dogecoin.tar.gz" | sha256sum -c - \
 && tar -xzf /tmp/dogecoin.tar.gz -C /opt \
 && ln -s /opt/dogecoin-${VERSION}/bin/dogecoind /opt/dogecoin-${VERSION}/bin/dogecoin-cli /usr/local/bin/ \
 && rm /tmp/dogecoin.tar.gz \
 && apt-get purge -y curl && apt-get autoremove -y && rm -rf /var/lib/apt/lists/* \
 && useradd --create-home --uid 1000 coin \
 && mkdir /data && chown coin:coin /data

USER coin
VOLUME /data
ENTRYPOINT ["dogecoind", "-datadir=/data", "-printtoconsole"]
