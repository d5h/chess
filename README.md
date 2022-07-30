# Running

To make things out-of-the-box easy, I've provided a Dockerfile to get a dev environment
running.

```bash
# Build a dev image
docker build -t chess .

# Do dev in it
docker run \
    --mount "type=bind,src=$(pwd),dst=/src/chess" \
    --mount type=volume,src=chess-cargo-home,dst=/cargo/home \
    --mount type=volume,src=chess-cargo-target,dst=/cargo/target \
    -p 4000:4000 \
    -it \
    chess
```

Once inside the container, you can build and run the project like this:

```bash
# Build the project
cargo build --target wasm32-unknown-unknown --release

# Run the UI
cd ui
cp $CARGO_TARGET_DIR/wasm32-unknown-unknown/release/chess-ui.wasm . \
    && basic-http-server -a 0.0.0.0:4000
```
