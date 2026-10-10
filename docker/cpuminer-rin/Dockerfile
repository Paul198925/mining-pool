# CPU test miner for RinHash: the Rincoin project's own cpuminer-opt fork.
# Built for the CPU it is built on (-march=native), so this image is for local tests only.
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates git build-essential automake autoconf pkg-config \
      libcurl4-openssl-dev libjansson-dev libssl-dev libgmp-dev zlib1g-dev \
 && rm -rf /var/lib/apt/lists/* \
 && git clone https://github.com/Rin-coin/cpuminer-opt-rin /src \
 && cd /src && git checkout d1d0784 \
 # The repository lacks the (empty) config header its bundled Keccak code includes.
 && touch algo/rinhash/sha3/config.h \
 && ./autogen.sh && CFLAGS="-O3 -march=native -Wall" ./configure --with-curl && make -j"$(nproc)"
ENTRYPOINT ["/src/cpuminer"]
