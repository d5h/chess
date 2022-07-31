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
    -p 4001:4001 \
    -it \
    chess
```

Once inside the container, you can build and run the project like this:

```bash
# Build the project
cargo build --release

# Run the UI
cp $CARGO_TARGET_DIR/wasm32-unknown-unknown/release/chess-ui.wasm ui \
    && basic-http-server -a 0.0.0.0:4000 ui

# Run the server
```

If you're using VS Code, install the
[remote container extension](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers)
and attach to the running container (the button is in the bottom left corner). Also
install the [rust analyzer extension](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).