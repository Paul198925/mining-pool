# The release binary is built on Ubuntu 24.04 and dynamically linked, hence this base.
FROM ubuntu:24.04

ARG VERSION=1.1.0rc1
ARG TAG=v1.1.0-rc1
# From the release's signed SHA256SUMS.
ARG SHA256=2e5303e384a4822f0ea250df85a64062ac58f65b6fff91bf182bbd291dee3e42

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl libminiupnpc17 libzmq5 libfmt9 \
      libboost-filesystem1.83.0 libboost-thread1.83.0 libevent-2.1-7t64 libevent-pthreads-2.1-7t64 libsqlite3-0 \
 && curl -fsSL -o /tmp/rincoin.tar.gz \
      "https://github.com/Rin-coin/rincoin/releases/download/${TAG}/rincoin-${VERSION}-x86_64-linux-gnu.tar.gz" \
 && echo "${SHA256}  /tmp/rincoin.tar.gz" | sha256sum -c - \
 && tar -xzf /tmp/rincoin.tar.gz -C /usr/local bin/rincoind bin/rincoin-cli bin/rincoin-wallet \
 && rm /tmp/rincoin.tar.gz \
 && apt-get purge -y curl && apt-get autoremove -y && rm -rf /var/lib/apt/lists/* \
 && userdel -r ubuntu && useradd --create-home --uid 1000 coin \
 && mkdir /data && chown coin:coin /data

COPY --chmod=755 entrypoint.sh /usr/local/bin/pool-entrypoint
USER coin
VOLUME /data
ENTRYPOINT ["pool-entrypoint"]
