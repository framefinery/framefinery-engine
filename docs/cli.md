# FrameFinery CLI

The installed command is `ff`.

```sh
cargo install framefinery
ff --help
ff --version
```

Focused help pages are built into the binary:

```sh
ff --help codecs
ff --help filters
ff --help filters pattern
ff --help pixfmt
ff --help settings
ff --help settings qp
ff --help presets
```

The main option list is generated from the facade crate's CLI option inventory.
Codec, filter, and setting detail pages are generated from compiled manifests,
so disabled codecs or filters are not advertised by the binary.

## Encode

The primary command shape is:

```sh
ff encode [<input>] [input-options] [--filter <spec>] \
  --encode <codec:output> [output-options]
```

`--encode` combines the codec and output path:

```sh
ff encode input.y4m --encode av2:output.obu --set qp=24 --psnr
ff encode input.y4m --encode vvc:output.vvc --set lossless --recon recon.yuv
```

Raw inputs need dimensions and pixel format unless the filename already carries
metadata:

```sh
ff encode input.yuv --video 1920x1080:yuv420p8 --fps 30 --frames 50 \
  --encode av2:output.obu --set qp=24 --psnr
```

Supported filename metadata uses:

```text
*_<WxH>[_<fps>][_<frames>f][_<pixfmt>].yuv
```

For example:

```sh
ff encode clip_1920x1080_30_50f_yuv444p8.yuv \
  --encode av2:output.obu --set lossless
```

Y4M headers provide width, height, frame rate, and planar YUV format. If
`--frames` is omitted for a file input, encoding stops at EOF.

Source filters such as `pattern=checker` can generate raw frames directly:

```sh
ff encode --filter pattern=color_blocks --video 3840x2160:gbrp8 \
  --fps 60 --frames 3600 --encode av2:/dev/null --set qp=24
```

The source and transform filter path is pull-based and should not buffer the
whole generated stream in memory.

During encode, `ff` prints one progress line per encoded frame on stderr. The
line reports frame position, elapsed wall time, average FPS, per-frame encode
time, per-frame bytes, and cumulative output bytes. When `--psnr` is present,
the same line also includes per-plane and aggregate PSNR. Use `--no-progress`
to suppress these per-frame lines when another process is driving the CLI.

## Settings

Encoder settings use repeated `--set key[=value]` arguments. Bare keys imply
`true`.

Common settings:

```sh
--set lossless
--set qp=24
--set gop=0
--set fast-search=lossless-speed
--set profile=auto
```

`--set lossless` and `--set qp=<1..255>` are mutually exclusive.
Temporal prediction defaults to `gop=-1`, meaning one intra frame followed by
unbounded predictive frames. Use `--set gop=0` for intra-only coding, or a
positive value such as `--set gop=30` to insert an intra frame every 30 frames.
VVC defaults to `profile=auto`, which selects the lowest 4:4:4-capable profile
for the input bit depth so palette and related screen-content tools remain
legal. Use explicit VVC values such as `profile=main-10` only when the tighter
profile is required.
Run `ff --help settings` or `ff --help settings <name>` for the accepted
settings, defaults, and full spec contracts.

## Metrics

Use `--psnr` to print per-frame PSNR from the encoder's in-memory
reconstruction without writing a reconstruction stream:

```sh
ff encode input.y4m --encode av2:out.obu --set qp=24 --psnr
```

Use `--recon <path>` only when you need the raw internal reconstruction for
debugging or reference-decoder validation:

```sh
ff encode input.y4m --encode vvc:out.vvc --set lossless --recon out_recon.yuv
```

## Validation

Local release-oriented checks are driven by Makefile targets:

```sh
make release-check
make validate-release-aomctc AOMCTC_ROOT=/path/to/aomctc
make pre-release-validation AOMCTC_ROOT=/path/to/aomctc
make release-performance-table
```

`validate-release-aomctc` reads the local AOM CTC A5/B2 Y4M files directly
from `AOMCTC_ROOT`. It does not decompress the optional B1 archive and does not
create raw source copies.
