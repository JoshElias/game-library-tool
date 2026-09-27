# Build only from scripts/export-web-build-context.py output, not the repo root.
FROM rust:1.88-bookworm AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/game-library/Cargo.toml crates/game-library/Cargo.toml
COPY crates/game-library/src crates/game-library/src
COPY crates/game-library/scripts/steam_inventory.py crates/game-library/scripts/steam_inventory.py
COPY crates/game-library-web/Cargo.toml crates/game-library-web/Cargo.toml
COPY crates/game-library-web/src crates/game-library-web/src
COPY crates/game-library-web/templates crates/game-library-web/templates
COPY crates/game-library-web/static crates/game-library-web/static
COPY crates/game-library-web/migrations crates/game-library-web/migrations
RUN cargo build --locked --release -p game-library-web

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl openssh-client python3 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /build/target/release/game-library-web /usr/local/bin/game-library-web
COPY crates/game-library-web/container-entrypoint.sh /usr/local/bin/game-library-entrypoint
RUN chmod 0755 /usr/local/bin/game-library-web /usr/local/bin/game-library-entrypoint
ENV GAME_LIBRARY_WEB_BIND=0.0.0.0:8787
EXPOSE 8787
HEALTHCHECK --interval=30s --timeout=5s CMD curl -fsS http://127.0.0.1:8787/health || exit 1
# Inventory, recipes, DB URL file, and any endpoint SSH helper are runtime
# inputs. There are deliberately no baked host lists or recipe COPY lines.
ARG SOURCE_REVISION
ARG SOURCE_URL
LABEL org.opencontainers.image.revision="${SOURCE_REVISION}"
LABEL org.opencontainers.image.source="${SOURCE_URL}"
USER 65534:65534
ENTRYPOINT ["/bin/sh", "/usr/local/bin/game-library-entrypoint"]
