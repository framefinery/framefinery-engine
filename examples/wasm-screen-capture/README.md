# FrameFinery WASM Screen Capture Example

This is a target-practice browser integration. It is not a published WASM or
npm package yet.

The page captures screen-share frames through `getDisplayMedia`, center-crops
one source-pixel-exact frame at the selected geometry without scaling, copies
RGBA bytes into the FrameFinery WASM target, immediately calls the Rust-style
`encode_frame` path, streams the encoded bytes from that call over a WebSocket
to the local server, and then schedules the next capture. If encoding or upload
backpressure takes longer than the requested frame interval, the next capture
is delayed instead of queuing raw frames in memory.

The local Python server exposes `/stream` as a dependency-free WebSocket
receiver and writes binary messages to a `.part` file as they arrive. When the
browser sends the final frame count, the server renames the partial file to the
normal `screen_capture_<codec>_<frames>f_<timestamp>` filename.

Because the native AV2/VVC sessions are still buffered internally, each browser
`encode_frame` call currently produces an immediate one-frame encode through
the native API shape rather than one long predictive encoder session. The demo
transport is stream-first; replacing the one-frame bridge with a predictive
incremental session should happen in the Rust encoder API rather than in the
browser upload layer.

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

Successful streamed captures are written under:

```text
verification/generated/wasm_screen_capture/
```

The committed Rust target intentionally uses a small raw WASM ABI instead of
`wasm-bindgen`. That keeps this experiment dependency-free and makes the
published FrameFinery crates prove `wasm32-unknown-unknown` compatibility
without committing to a final web packaging strategy.
