# Build: docker build --build-arg BUILDKIT_INLINE_CACHE=1 -t chess-deploy .
# Run: docker run -it -p 58597:58597 chess-deploy

FROM chess as build

ENV CARGO_HOME=/cargo/home
ENV CARGO_TARGET_DIR=/cargo/target

COPY . .
RUN --mount=type=cache,target=/cargo/home \
    --mount=type=cache,target=/cargo/target \
    cargo build --release && \
    strip $CARGO_TARGET_DIR/release/server && \
    cp $CARGO_TARGET_DIR/release/server /usr/local/bin/chess-server && \
    cp --remove-destination $CARGO_TARGET_DIR/wasm32-unknown-unknown/release/*.wasm /srv/chess

# ---

FROM debian:sid-slim

ENV CARGO_TARGET_DIR=/cargo/target

RUN mkdir -p /src/chess /srv/chess
COPY --from=build /src/chess /src/chess
COPY --from=build /srv/chess /srv/chess
COPY --from=build /usr/local/bin/chess-server /usr/local/bin

EXPOSE 58597
ENV RUST_LOG=debug

ENTRYPOINT ["chess-server"]
