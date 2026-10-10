# The release binary is dynamically linked against glibc 2.38+, hence a newer base than the other nodes.
FROM ubuntu:24.04

ARG VERSION=0.6.8.10
# Digest of the release asset as reported by GitHub.
ARG SHA256=fc0dff7ad8d8c10e447e263de5c1ded1256581800e626ae915d92510fea6ec15

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl unzip \
 && curl -fsSL -o /tmp/rod.zip \
      "https://github.com/SpaceXpanse/rod-core-wallet/releases/download/v${VERSION}/spacexpanse-${VERSION}-linux64.zip" \
 && echo "${SHA256}  /tmp/rod.zip" | sha256sum -c - \
 && unzip -j /tmp/rod.zip spacexpansed spacexpanse-cli spacexpanse-wallet -d /usr/local/bin \
 && chmod +x /usr/local/bin/spacexpansed /usr/local/bin/spacexpanse-cli /usr/local/bin/spacexpanse-wallet \
 && rm /tmp/rod.zip \
 && apt-get purge -y curl unzip && apt-get autoremove -y && rm -rf /var/lib/apt/lists/* \
 && userdel -r ubuntu && useradd --create-home --uid 1000 coin \
 && mkdir /data && chown coin:coin /data

COPY --chmod=755 entrypoint.sh /usr/local/bin/pool-entrypoint
USER coin
VOLUME /data
ENTRYPOINT ["pool-entrypoint"]
