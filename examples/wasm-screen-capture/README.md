# FrameFinery WASM Screen Capture Example

This is a target-practice browser integration. It is not a published WASM or
npm package yet.

The page captures a bounded screen-share sequence through `getDisplayMedia`,
draws frames into a canvas at the selected geometry, copies RGBA bytes into the
FrameFinery WASM target, encodes the captured frames as AV2 or VVC, and posts
the elementary stream to the local Python server.

Build and run:

```sh
rustup target add wasm32-unknown-unknown
make wasm-build
make wasm-screen-demo
```

Open:

```text
http://127.0.0.1:8008/
```

Successful uploads are written under:

```text
verification/generated/wasm_screen_capture/
```

The committed Rust target intentionally uses a small raw WASM ABI instead of
`wasm-bindgen`. That keeps this experiment dependency-free and makes the
published FrameFinery crates prove `wasm32-unknown-unknown` compatibility
without committing to a final web packaging strategy.
