# framefinery-core

`framefinery-core` contains the reusable media primitives behind FrameFinery.
It is intentionally codec-agnostic: codec crates and applications can share
frame buffers, pixel-format handling, pipeline traits, and small reusable
filters without depending on AV2, VVC, or the `ff` command-line frontend.

## API Surface

The crate is organized around a simple media pipeline:

```text
Source -> Filter -> Encoder -> Sink
```

Important public types:

- `FrameInfo`, `Frame`, `PixelFormat`, `SampleBitDepth`, and
  `ChromaSampling` describe raw frames and validated buffer sizes.
- `Source`, `Filter`, `Encoder`, and `Sink` are small traits for pipeline
  stages.
- `run_frame_filter_pipeline` and `run_frame_encode_pipeline` connect stages
  and report frame/packet counts.
- `IdentityFilter` is the no-op transform filter.
- `PatternSource` and `PatternKind` generate deterministic raw-video test
  frames.
- `FILTERS` and `filter_manifest` expose the reusable filter catalog used by
  the `ff` CLI.
- `FilterSpecManifest` describes each filter's accepted spec forms,
  parameters, examples, and notes.

## Filters

Filter manifests live in this crate so library users and the CLI discover the
same stages:

```rust
use framefinery_core::{filter_manifest, FilterStageKind};

let filter = filter_manifest("identity").expect("identity filter");
assert_eq!(filter.stage, FilterStageKind::Transform);
assert_eq!(filter.spec.forms[0].syntax, "identity");
```

Filter spec metadata is structured so applications can render their own help
pages without copying CLI strings:

```rust
use framefinery_core::filter_spec_manifest;

let spec = filter_spec_manifest("pattern").expect("pattern spec");
assert_eq!(spec.forms[0].syntax, "pattern=<name>");
```

`PatternSource` is modeled as a source filter because it creates frames instead
of transforming an input stream:

```rust
use framefinery_core::{FrameInfo, PatternKind, PatternSource, PixelFormat, Source};

let info = FrameInfo::new(16, 16, PixelFormat::Yuv420p8)?;
let mut source = PatternSource::new(info, PatternKind::Checker, 1)?;
let frame = source.pull()?.expect("one generated frame");
assert_eq!(frame.info(), info);
# Ok::<(), framefinery_core::MediaError>(())
```

Current pattern output supports `yuv420p8` and `yuv444p8`. The accepted pattern
names are `black`, `checker`, `gradient`, and `color_blocks`; `blocks` is
accepted as a short alias for `color_blocks`.

`crop` and `scale` are currently manifest scaffolds. They are listed so the CLI
and library API reserve stable names, but executable filter implementations
still need to be added.

## Features

All filter features are enabled by default for direct `framefinery-core` users:

- `filter-pattern`
- `filter-identity`
- `filter-crop`
- `filter-scale`

Applications that want a smaller compiled surface can disable default features
and enable individual filter implementations:

```toml
framefinery-core = {
  version = "0.0.2",
  default-features = false,
  features = ["filter-identity"]
}
```

The filter manifest is always present so tools can report unavailable stages.
The concrete implementation types and helpers, such as `IdentityFilter` and
`PatternSource`, are compiled only when their matching `filter-*` feature is
enabled.

## Errors

Public fallible APIs return `framefinery_core::Result<T>`, an alias for
`Result<T, MediaError>`. Buffer sizes and frame dimensions are validated before
frame construction or pattern generation.
