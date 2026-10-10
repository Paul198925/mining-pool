# Reference CPU miner (pooler/cpuminer) for end-to-end tests against the stratum server.
FROM debian:bookworm-slim AS build
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates git build-essential automake autoconf pkg-config libcurl4-openssl-dev \
 && git clone --depth 1 --branch v2.5.1 https://github.com/pooler/cpuminer /src \
 && cd /src && ./autogen.sh && ./configure CFLAGS="-O2" && make

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends libcurl4 ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/minerd /usr/local/bin/minerd
ENTRYPOINT ["minerd"]
