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

Native AV2/VVC source and owned-frame sessions now share persistent per-codec
state and emit complete frames incrementally. This demo's WASM wrapper still
creates a fresh encoder for each frame and retains cumulative encoded output.
Those wrapper behaviors are open gaps against the
[API streaming contract](../../docs/api-v0.md#encoder-sessions). The next wrapper
change must reuse the generic registry/session API, drain each returned step and
bound transport queues. The current demo does not yet provide one continuous
predictive stream, and native session tests do not validate browser behavior.

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
