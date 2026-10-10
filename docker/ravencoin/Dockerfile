FROM debian:bookworm-slim

ARG VERSION=4.8.0-225491298
ARG TAG=v4.8.0
# The digest GitHub publishes for the release asset.
ARG SHA256=cb359b6a5b42e47068cd655231484fcc763d2f79eae5ea318b029c704a4dc020

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && curl -fsSL -o /tmp/raven.tar.gz \
      "https://github.com/RavenProject/Ravencoin/releases/download/${TAG}/raven-${VERSION}-x86_64-linux-gnu.tar.gz" \
 && echo "${SHA256}  /tmp/raven.tar.gz" | sha256sum -c - \
 && mkdir /opt/raven && tar -xzf /tmp/raven.tar.gz -C /opt/raven \
 && ln -s "$(find /opt/raven -type f -name ravend)" "$(find /opt/raven -type f -name raven-cli)" /usr/local/bin/ \
 && rm /tmp/raven.tar.gz \
 && apt-get purge -y curl && apt-get autoremove -y && rm -rf /var/lib/apt/lists/* \
 && useradd --create-home --uid 1000 coin \
 && mkdir /data && chown coin:coin /data \
 && ravend -version | head -1

USER coin
VOLUME /data
ENTRYPOINT ["ravend", "-datadir=/data", "-printtoconsole"]
