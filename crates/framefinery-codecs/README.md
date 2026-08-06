# framefinery-codecs

`framefinery-codecs` contains the experimental software encoder
implementations used by FrameFinery. It depends on `framefinery-core` for the
codec-neutral frame, config, source, metrics, and manifest types.

The crate currently provides local AV2 and VVC encoder models. These
implementations are research and validation code, not codec-specific public API
contracts. Normal applications should use the generic registry helpers exposed
by `framefinery` or this crate:

- `ENCODERS` lists compiled encoder manifests.
- `find_encoder_manifest(name)` returns discovery metadata.
- `create_encoder(config)` creates a frame-session encoder for
  `config.codec`.
- `encode_frame(config, frame)` encodes one owned frame.
- `encode_source(&config, source, output, recon, metrics)` pulls raw frames
  from a `RawVideoFrameSource` without preloading a whole stream.

## Features

The crate has no default codecs when used directly. Enable the codec families
you want:

```toml
framefinery-codecs = {
  version = "0.0.2",
  features = ["av2", "vvc"]
}
```

Feature flags:

- `av2`: compile the local experimental AV2 encoder.
- `vvc`: compile the local experimental VVC encoder.
- `bench-internals`: expose hidden benchmark-only internals.
- `av2-stats`, `av2-lossy-stats`, `av2-sb-bit-profile`, and `vvc-stats`:
  compile gated instrumentation used by local profiling workflows.
- `dead-code-audit`: remove the normal dead-code warning suppression used when
  building only part of the codec set.

The user-facing `framefinery` package enables AV2 and VVC by default, so most
users should depend on `framefinery` unless they specifically want the narrower
codec crate.

## API Stability

FrameFinery is still in the `0.0.x` line. Registry helpers and core media types
are the intended integration surface, but names and behavior may still break
before `0.1.0`. Codec modules and hidden benchmark exports should be treated as
implementation details.

## Licensing

The source license is declared in the Cargo manifest. Codec patent obligations
are separate from source-code copyright licensing; evaluate codec deployment
obligations for your use case and jurisdiction.
