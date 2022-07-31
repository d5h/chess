FROM rust

RUN apt-get update
RUN apt-get install -y libasound2-dev

RUN rustup default nightly
RUN rustup target add wasm32-unknown-unknown
RUN rustup component add rustfmt
RUN cargo install basic-http-server

WORKDIR /src/chess
ENV CARGO_HOME=/cargo/home
ENV CARGO_TARGET_DIR=/cargo/target

ENTRYPOINT ["bash"]
