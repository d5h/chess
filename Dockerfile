FROM rust

RUN apt-get update
RUN apt-get install -y libasound2-dev

RUN rustup default nightly
RUN rustup target add wasm32-unknown-unknown
RUN rustup component add rustfmt

WORKDIR /src/chess
ENV CARGO_HOME=/cargo/home
ENV CARGO_TARGET_DIR=/cargo/target

RUN mkdir -p /srv/chess \
    && ln -s /src/chess/ui/index.html /srv/chess \
    && ln -s /src/chess/ui/assets /srv/chess \
    && ln -s $CARGO_TARGET_DIR/wasm32-unknown-unknown/release/chess-ui.wasm /srv/chess

ENTRYPOINT ["bash"]
