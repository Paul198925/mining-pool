# CPU test miner for Xaya-style Neoscrypt (the "neoscrypt-xaya" algorithm of GPU miners).
# No CPU miner ships it, so this is tpruvot/cpuminer-multi with the one difference patched in:
# the merkle root words of the header are byte-swapped once more (compare xaya/ccminer).
FROM debian:bookworm-slim AS build
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates git build-essential automake autoconf pkg-config \
      libcurl4-openssl-dev libjansson-dev libssl-dev libgmp-dev zlib1g-dev \
 && git clone https://github.com/tpruvot/cpuminer-multi /src \
 && cd /src && git checkout d2927ed \
 && sed -i 's|work->data\[9 + i\] = be32dec((uint32_t \*) merkle_root + i);|work->data[9 + i] = le32dec((uint32_t *) merkle_root + i);|' cpu-miner.c \
 && grep -q 'le32dec((uint32_t \*) merkle_root + i)' cpu-miner.c \
 && ./autogen.sh && ./configure CFLAGS="-O2 -fcommon" --with-crypto --with-curl && make

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libcurl4 libjansson4 libssl3 libgmp10 zlib1g \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/cpuminer /usr/local/bin/cpuminer
ENTRYPOINT ["cpuminer"]
