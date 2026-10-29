FROM node:22-bookworm-slim AS frontend
WORKDIR /build/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

FROM rust:1.99-bookworm AS backend
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
COPY profiles/ ./profiles/
RUN cargo build --locked --release

FROM docker:28-cli AS docker-cli

FROM debian:bookworm-slim AS updater
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl git tar gzip \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /control /updates /app-data /usr/local/libexec/docker/cli-plugins \
    && chown 10001:10001 /app-data
COPY --from=docker-cli /usr/local/bin/docker /usr/local/bin/docker
COPY --from=docker-cli /usr/local/libexec/docker/cli-plugins/docker-buildx /usr/local/libexec/docker/cli-plugins/docker-buildx
COPY --from=backend /build/target/release/zakoai-updater /app/zakoai-updater
ENV UPDATER_STATE_DIR=/updates \
    UPDATER_DATA_DIR=/app-data \
    UPDATER_TOKEN_FILE=/control/token
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=6 \
    CMD curl --fail --silent --show-error http://127.0.0.1:9000/health || exit 1
ENTRYPOINT ["/app/zakoai-updater"]

FROM debian:bookworm-slim AS runtime
LABEL org.opencontainers.image.version="0.13.2"
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 zako \
    && useradd --uid 10001 --gid zako --no-create-home --shell /usr/sbin/nologin zako \
    && mkdir -p /app/web/dist /app/licenses /data \
    && chown -R zako:zako /app /data
WORKDIR /app
COPY --from=backend --chown=zako:zako /build/target/release/zakoai-proxy /app/zakoai-proxy
COPY --from=frontend --chown=zako:zako /build/web/dist/ /app/web/dist/
COPY --from=frontend --chown=zako:zako /build/web/dist/assets/third-party-licenses.txt /app/licenses/frontend-notices.txt
COPY --chown=zako:zako LICENSE third_party/gpt-instruct.LICENSE third_party/coldcoffee.LICENSE /app/licenses/
COPY --chown=zako:zako third_party/kimi-break/LICENSE /app/licenses/kimi-break.LICENSE
COPY --chown=zako:zako third_party/ai-jailbreak/LICENSE /app/licenses/ai-jailbreak.LICENSE
COPY --chown=zako:zako third_party/ai-jailbreak/NOTICE.md /app/licenses/ai-jailbreak.NOTICE.md
COPY --chown=zako:zako third_party/hanshuang/NOTICE.md /app/licenses/hanshuang.NOTICE.md
ENV DATA_DIR=/data \
    WEB_DIR=/app/web/dist \
    BIND_ADDR=0.0.0.0:1990 \
    RUST_LOG=zakoai_proxy=info
USER 10001:10001
VOLUME ["/data"]
EXPOSE 1990
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD curl --fail --silent --show-error http://127.0.0.1:1990/health || exit 1
ENTRYPOINT ["/app/zakoai-proxy"]
