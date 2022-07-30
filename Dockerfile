FROM rust

RUN rustup default nightly
RUN rustup target add wasm32-unknown-unknown
RUN cargo install basic-http-server

WORKDIR /src/chess
ENV CARGO_HOME=/cargo/home
ENV CARGO_TARGET_DIR=/cargo/target

ENTRYPOINT ["bash"]
