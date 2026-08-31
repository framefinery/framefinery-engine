# Codec maintenance audit

This document records the current structural cleanup checkpoint for the codec
implementations. It is deliberately separate from bitrate comparisons: its
purpose is to track maintainability, feature accountability, and validation
invariants while the encoders continue to evolve.

## Invariants for every refactor

- Keep lossless and lossy coding on the same traversal and reconstruction
  pipeline. Differences belong in the deepest mode or candidate selection
  layer.
- Keep pixel-format and profile restrictions at the mode-selection boundary
  where possible. Do not duplicate syntax or reconstruction implementations
  for a format or profile.
- Preserve safe Rust and validate dimensions, plane lengths, and arithmetic
  behavior at public and allocation boundaries.
- Require reference-decoder reconstruction agreement for generated streams.
  Lossless streams must reconstruct exactly; lossy streams must retain their
  recorded quality measurements.
- Run formatting, the complete codec tests, feature-matrix checks, the
  dead-code audit, performance-focused Clippy, required-reference smoke
  validation, and the representative lossless/lossy metric matrix before each
  cleanup commit.

## Completed structural cleanups

The following changes were behavior-preserving and independently validated:

- VVC CABAC statistics data/sink code was separated from statistics analysis.
- VVC intra MPM and mode-syntax helpers were separated from CTU traversal and
  neighbour-state orchestration.
- VVC predictive encoding helpers were separated from the main encoder path.
- VVC CTU mode selection was separated from CTU orchestration helpers.
- VVC CCLM prediction was separated from the remaining prediction kernels.
- VVC intra top/left reference-edge collection was separated from prediction
  kernels while retaining shared availability handling.
- VVC reference-sample availability, fallback, and top-left access are now
  isolated as shared helpers for prediction and reference-edge collection.
- VVC reference filtering and DC prediction arithmetic now live in a dedicated
  helper module shared by the common luma/chroma prediction paths.
- VVC plane-availability storage and bounds-checked lookup now live with the
  shared reference-access helpers.
- VVC planar/DC and angular PDPC arithmetic now lives in a dedicated helper
  module used by the common prediction paths.
- VVC angular zero-reference and multi-reference-line construction now lives
  in a dedicated reference-builder module.
- VVC transform-skip residual placement and reconstruction now share one
  parameterized kernel across luma/chroma and identity/QP/table paths.
- VVC BDPCM transform-skip inverse traversal and reconstruction now share the
  same parameterized kernel across luma and chroma paths.
- VVC visible luma and chroma reconstruction now share one clipped
  plane-writing loop, with subsampling handled only at the caller boundary.
- VVC visible reconstruction wrappers and their shared clipped plane writer now
  live in a dedicated reconstruction module, separate from prediction kernels.
- VVC BDPCM prediction and residual adapters now live in a dedicated module,
  retaining one shared plane predictor for luma and chroma.
- VVC luma and chroma transform-skip SSE scoring now share one kernel,
  including the common BDPCM accumulation, clipped tail handling, and
  format-neutral residual accounting utilities.
- VVC luma and chroma transform-skip AC extraction now share one bounded
  level-placement and nonzero-tracking kernel.
- VVC finalized residual-block and RD-score value types now live in the shared
  quantization types module used by luma, chroma, and CTU selection.
- VVC finalized luma and chroma TU records now live beside the shared
  quantization records used by CTU orchestration and tracing.
- VVC DC, planar, coarse-directional, and refined-directional luma candidates
  now share one prediction-and-scoring evaluator, strict-winner state, and
  ordered cost recording. Their complete candidate search and deepest policy
  gates live in the focused luma-search module, while reconstruction, RD
  refinement, and finalization remain on the common CTU path.
- VVC luma and chroma search gates now live beside their respective search
  implementations; the generic mode-selection helper grab bag was removed.
- AV2 visible/coded geometry padding and cropping now lives in a dedicated
  geometry module, keeping format conversion separate from encode traversal.
- VVC scalar residual energy, sum, and signed rounding helpers now live in a
  dedicated transform-math module separate from transform selection.
- AV2 lossy 4:2:2 and 4:4:4 chroma residual writers now live in a dedicated
  chroma-residual module while retaining the shared lossy analysis path.
- VVC DCT/DST basis lookup and MTS normalization now live in a dedicated
  transform-basis module separate from quantization and dequantization.
- VVC transform dequantization parameters and DC-only reconstruction now live
  in a dedicated dequant module beside the transform-basis helpers.
- VVC inverse transform and dequantized-level reconstruction now live in a
  dedicated transform-inverse module separate from forward quantization.
- VVC inter-motion state, AMVP candidate selection, and HMVP retention now
  live in a dedicated inter-motion module separate from CABAC emission.
- VVC luma intra-mode syntax and BDPCM/MIP/MRL/ISP eligibility checks now live
  in a dedicated mode-syntax module on the shared CABAC generator.
- VVC luma post-residual LFNST and MTS syntax gates now live in a dedicated
  residual-tools module on the shared CABAC generator.
- VVC chroma leaf/tool legality, split contexts, and BT direction selection now
  live in a dedicated chroma mode-selection module on the shared generator.
- VVC chroma transform-leaf, BDPCM, and intra-prediction syntax now live in a
  dedicated chroma-leaf module on the shared generator.
- VVC chroma coefficient emission now lives in a dedicated residual-syntax
  module while retaining the common CABAC coefficient writer.
- VVC luma CBF, transform-unit dispatch, and stored-coefficient emission now
  live in a dedicated luma-residual module on the shared generator.
- AV2 lossless intra/inter residual coefficient writers now live in a dedicated
  lossless-residual module, separate from lossy residual selection.
- AV2 palette-specific luma residual emission now lives in a dedicated
  palette-residual module, separate from the general residual writer.
- AV2 lossy subsampled scoring was separated from its materialization path.
- AV2 lossy directional/IDIF/smooth edge collection and score-neighbour
  predictor adapters now live in a focused prediction sibling, reducing the
  general scoring module while preserving the shared analysis path.
- AV2 lossless subsampled tile construction and plane-coordinate mapping now
  live in a dedicated state module, separate from prediction and scoring.
- AV2 source-backed lossless DPCM now uses the shared edge-predictor residual
  kernel instead of a parallel hand-written materialization loop.
- AV2 source-backed lossless predictor adapters now live in a dedicated module,
  separate from ordinary reconstruction and score-path logic.
- AV2 lossy subsampled quantized, residual, DPCM, and source-copy writes now
  share one clipped reconstruction traversal with mode-specific sample logic.
- AV2 lossy 4:2:2 and 4:4:4 chroma leaf reconstruction now shares one clipped
  writer, with only the transform residual stride supplied by each caller.
- AV2 lossy 4:4:4 chroma analysis and motion-reference adapters now live in a
  dedicated layer around the shared 8x8 analysis kernel.
- AV2 lossy wide-transform candidate builders now live in a dedicated module,
  separate from ordinary 4x4 candidate selection.
- AV2 lossy DPCM analysis and candidate construction now live in a dedicated
  module, separate from regular transform candidates and leaf orchestration.
- AV2 lossy regular 4x4 candidate builders now live in a dedicated module,
  separate from mode-decision orchestration and wide-transform adapters.
- AV2 lossy intra/inter TXB analysis now shares a dedicated finalization module,
  keeping both analysis paths on one residual and distortion contract.
- AV2 lossless subsampled luma/chroma residual mode dispatch now shares one
  source-backed/reconstructed-reference selector, keeping fast-mode gates at
  mode selection while reusing the common residual materialization path.
- AV2 lossless intra residual scoring and materialization now share one mode
  and predictor path, with only the reconstructed-versus-score neighbour source
  selected where predictor edges are read.
- AV2 lossless BDPCM scoring and materialization now use the same neighbour
  reference policy and edge-predictor adapter instead of separate horizontal
  and vertical score wrappers.
- AV2 fast lossless BDPCM now uses the shared residual path directly; its
  reconstructed-reference policy already resolves to source samples in fast
  mode, so the source-backed mode branch and duplicate adapter were removed.
- AV2 fast lossless DC/H/V residuals now use the shared intra-prediction
  kernel with a preloaded 4x4 source block, preserving the shortcut's memory
  access pattern while removing its duplicate residual loops and mode gates.
- AV2 lossless directional-IDIF residuals now preload the same 4x4 source
  block directly, removing a single-caller generic source callback and wrapper
  while retaining the shared reconstructed-versus-score edge policy.
- AV2 lossless residual materialization and coefficient scoring now share one
  luma/chroma/BDPCM mode dispatcher, with only the deepest reconstructed-versus-
  score reference policy differing between callers.
- AV2 lossless intra and inter chroma residual emission now shares one U/V TXB
  traversal and entropy-context update path, with residual generation and FSC
  syntax policy supplied only at the deepest emission boundary.
- AV2 lossless intra and inter luma residual emission now shares one TXB
  traversal, reconstruction update, and entropy-context update path; each mode
  retains only its TXB-local residual generation and legal syntax selection.
- AV2 lossless directional, IDIF, and smooth edge collection now lives in a
  focused sibling module, reducing the general subsampled state implementation
  while preserving the existing availability and reconstructed/source policy.
- AV2 lossless and lossy IDIF prediction now shares one edge-buffer assembler;
  each state retains only its typed sample adapter and boundary policy.
- AV2 lossless and lossy directional above/left edge collection now shares one
  availability kernel; an explicit edge-limit policy preserves lossless plane
  bounds and lossy region bounds without separate algorithms.
- AV2 lossless and lossy smooth prediction now shares one region-clipped,
  superblock-aware edge-availability kernel; state adapters supply only plane
  coordinates, coded-neighbour lookup, and reconstructed/source sample policy.
- VVC residual mode scoring and fast-search gate helpers now live in a
  dedicated mode-selection helper module, separate from CTU orchestration while
  retaining the same deepest-level feature gates.
- AV2 all-inter entropy writers now live in a dedicated syntax module, keeping
  inter-only emission separate from intra, palette, and lossy tile planning.
- VVC luma and chroma visible transform-skip/BDPCM reconstruction now share one
  clipped sample writer, with subsampling reduced to wrapper geometry setup.
- AV2 lossy inter residual entropy emission now lives with the other inter-only
  writers, keeping intra/palette planning separate from inter leaf state.
- AV2 fixed-NEWMV inter entropy emission now shares the dedicated inter syntax
  module with global-MV and residual inter writers.
- AV2 mixed inter/intra lossless entropy emission now lives with the dedicated
  inter syntax writers, keeping its active-inter-leaf handling isolated from
  general tile-plan construction.
- AV2 fixed-tree intra/palette entropy emission now lives in a dedicated intra
  syntax module, separate from subsampled and inter writers.
- AV2 lossy subsampled entropy emission now lives in its own writer module,
  keeping lossy mode-cache and profiling state local to that coding path.
- VVC inter-skip CTU traversal and CABAC emission now live in a dedicated
  writer module, separate from the shared frame-state and mode emitters.
- VVC frame-level CTU CABAC state and neighbor bookkeeping now live in a
  dedicated state module, separate from per-operation syntax emission.
- VVC CTU operation dispatch now lives in a dedicated generator extension,
  keeping traversal routing separate from split, prediction, and residual
  syntax implementations.
- VVC CTU split-flag emission now lives in a dedicated syntax extension,
  separate from operation dispatch and mode/residual emission.
- VVC inter-prediction syntax helpers now live in a dedicated generator
  extension, separate from intra and transform-tree syntax.
- VVC SCC/IBC intra syntax now lives in a dedicated generator extension, with
  its feature gates and legality assertions preserved at mode selection.
- VVC single-tree chroma prediction dispatch now lives in a dedicated
  generator extension shared by intra and inter tree traversal.
- VVC single-tree residual orchestration now lives in a dedicated generator
  extension, retaining the common luma/chroma residual syntax path.
- VVC chroma-tree entry and neighbor setup now live in a dedicated generator
  extension, separate from recursive chroma partition traversal.
- VVC visible chroma QT/MTT recursion now lives in a dedicated traversal
  extension, separate from tree entry setup and leaf syntax.
- VVC chroma inter-skip subtree selection and leaf counting now live in a
  dedicated mode-selection extension, preserving the deepest-level gate.
- VVC implicit chroma boundary-child emission now lives in a dedicated
  geometry/traversal extension, separate from visible-tree recursion.
- VVC chroma QT/MTT split-flag syntax now lives in a dedicated syntax
  extension, shared by visible and implicit boundary traversal.
- VVC angular prediction parameter derivation and interpolation selection now
  live in a dedicated helper module, separate from the shared oriented block
  prediction and reference-sample kernels.
- VVC vertical and horizontal angular block traversal now lives in one
  dedicated oriented-prediction module, retaining the common reference and
  PDPC operations used by both orientations.
- VVC angular reference interpolation and bounded sample lookup now live in a
  dedicated sampling module, separate from oriented block traversal.
- VVC residual scan, coefficient-group, stride, and Rice-state helpers now
  live in a dedicated syntax-helper module, separate from CABAC emission.
- AV2 IDTX coefficient neighbor, magnitude, and sign-context helpers now live
  in a dedicated context module, separate from transform-block emission.
- VVC luma AC candidate dispatch and RD selection now live in a dedicated
  selection module, keeping candidate generation and reconstruction shared.
- VVC chroma DC fast-search, candidate evaluation, and exhaustive fallback now
  live in a dedicated search module, preserving the common reconstruction SSE
  contract.
- VVC direct chroma AC quantization and chroma-specific QP helpers now live in
  a dedicated candidate module, separate from shared transform math.
- VVC legacy-Hadamard and separable-transform luma AC candidate generation now
  lives in a dedicated candidate module, separate from candidate selection.
- VVC shared luma AC coefficient extent, Hadamard, and QP math now lives in a
  dedicated math module used by both luma candidate paths.
- VVC raster and compact stored coefficient access now lives in one dedicated
  accessor module shared by test and production residual syntax paths.
- VVC residual context configuration and transform-subblock geometry helpers
  now live in a dedicated configuration module, separate from CABAC emission.
- VVC residual pass-1 coefficient, subblock, and Rice context state now lives
  in a dedicated state module shared by direct and test syntax emission.
- VVC production CABAC emission and test symbol collection now share one
  stateful coefficient traversal, with sink-specific output only at emission.
- VVC context-model tests now invoke the shared direct coefficient emitter;
  the stale completed-state symbol replay implementation was removed.
- VVC BDPCM prediction, residual materialization, and directional scoring now
  share one clamped sample-delta helper with explicit i16 boundary tests.
- VVC CCLM block/pair orchestration, shared-luma scratch reuse, and layout
  derivation now live beside the dedicated CCLM math helpers instead of in the
  general luma/chroma prediction orchestration module.
- VVC angular reference preparation, zero-angle handling, and oriented
  prediction dispatch now live beside the focused angular parameter, reference,
  sampling, and PDPC helpers instead of in the general prediction module.
- VVC DC and planar reference preparation, interpolation, and PDPC application
  now share a focused core sibling while luma/chroma wrappers retain only
  geometry and subsampling adaptation.
- VVC quantization and explicit reconstruction now share one chroma-mode
  dispatcher for derived, explicit, and CCLM prediction; the duplicated
  reconstruction-only mode switch and lower-level re-exports were removed.
- AV2 lossless DC, horizontal, vertical, and BDPCM proxy scoring now lives in a
  dedicated score module, separate from residual materialization.
- AV2 lossy transform candidate selection and RD gates were separated from
  residual coefficient writing.
- AV2 transform-block entropy writers were separated from coefficient/context
  helpers.
- AV2 transform coefficient-map, lower-level, base-range, and neighbour lookup
  context math now lives in a dedicated sibling, separate from entropy
  emission order and syntax-field naming.
- AV2 regular intra and inter luma TXBs now share one coefficient traversal,
  sign pass, and entropy-context result; a small syntax policy selects only
  their distinct skip, EOB, and transform-type fields at emission.
- AV2 4x4, 8x8, and 4x8 chroma TXBs now share one reverse-scan sign,
  high-range, cumulative-level, and DC-context pass; geometry selects only the
  scan and syntax-field labels at that deepest boundary.
- AV2 4x4, 8x8, and 4x8 coefficient normalization and first/EOB bound
  discovery now share one const-generic scan pass; typed adapters retain the
  geometry-specific scans and invariant diagnostics.
- AV2 luma intra/inter and 4x4, 8x8, and 4x8 chroma EOB writers now share one
  token and extra-bit traversal; a focused syntax policy retains only each
  path's CDF, symbol count, static key, and field labels.
- AV2 4x4, 8x8, and 4x8 chroma coefficient-level writers now share one
  const-generic base/LF/EOB/low-range branch body; a compile-time geometry
  policy retains only field labels, sign labels, and neighbor-context
  derivation, and all three DC-only LF predicates now share one definition.
- AV2 4x4, 8x8, and 4x8 chroma TXBs now also share one complete skip, EOB,
  reverse coefficient, DC, sign, high-range, and entropy-context traversal;
  compile-time geometry policies retain only scan order, normalization
  diagnostics, and the syntax/context choices that differ by transform size.
- The unified AV2 chroma TXB policy and traversal now live in a focused
  360-line included sibling, reducing the shared transform-emission core from
  1,031 to 672 lines without changing module scope, visibility, or coding
  dispatch.
- AV2 inter-residual tracing was separated from residual coefficient writing,
  keeping diagnostics and feature-gated instrumentation out of the coding
  pipeline.
- AV2 regular transform-partition syntax writers now share a dedicated helper
  cluster for intra and inter paths.
- AV2 residual scalar rounding, quantization-step, and TXB end-of-block
  helpers now live in a shared residual-math module.
- AV2 lossless directional-angle and residual proxy scoring helpers are now
  isolated from the stateful lossless tile implementation.
- AV2 lossless and lossy DC prediction now share one edge-accumulation and
  rounding kernel, with only typed edge access supplied by each state path.
- AV2 horizontal, vertical, and above-left intra predictors now share common
  tile-edge fallback kernels across lossless and lossy state paths.
- AV2 lossy score-only DC, horizontal, vertical, and above-left prediction now
  uses those same reconstruction kernels, with only the score-neighbour sample
  callback differing from materialization.
- AV2 coefficient-proxy scoring and high-range symbol-cost helpers now live in
  a dedicated subsystem shared by residual candidate selection paths.
- AV2 lossless and lossy horizontal, vertical, and above-left predictor
  selection now share common edge-policy kernels.
- VVC luma and chroma regular intra prediction now share one Planar/DC/Angular
  dispatcher and one plane-region descriptor; luma MRL/filtering and chroma
  subsampling/4:2:2 angle mapping remain explicit only where their syntax and
  sample geometry differ.
- VVC derived, explicit, and CCLM chroma candidates now share one prediction
  helper, while every scored candidate shares one evaluator and strict-winner
  state. Their complete candidate loop and deepest legality/fast-search gates
  live in the focused chroma-search module; the unscored derived-only shortcut
  remains an explicit mode-selection result feeding the common RD path.
- VVC exact-inter, regular intra, RD-refined, MRL, BDPCM, and scored-inter luma
  selection now returns one selected-candidate record from a focused
  orchestration module. The exact-inter shortcut still bypasses unnecessary
  search, but it now feeds the same residual finalization, reconstruction,
  metadata, and trace tail as every other selected luma mode.
- VVC inter-derived, regular intra, RD-refined, and BDPCM chroma selection now
  returns one selected-candidate record from a focused orchestration module.
  Inter-derived prediction still bypasses unnecessary intra search, but now
  enters the same residual finalization, reconstruction, metadata, and trace
  tail as ordinary selected chroma modes.
- Accepted VVC luma and chroma temporal hints now return those same selected-
  candidate records instead of invoking parallel finalizers. Temporal hints
  retain their original priority and cheap-residual acceptance gates while
  sharing finalization, reconstruction, metadata, and trace handling with all
  other selected modes.
- VVC luma and chroma RD caches now use one checked residual-transfer operation
  for raw-mode selection and RD winner promotion. Read-only RD scoring keeps a
  single cache borrow, cache misses preserve the caller's scratch buffers, and
  only the miss path materializes residuals from the selected prediction.
- Every production VVC luma residual build now uses one node-aware scalar
  materializer across mode search, temporal hints, MRL/BDPCM candidates,
  explicit inter comparison, and RD refinement. General timing is recorded in
  that shared boundary, while RD-only timing remains gated at its deepest
  callers.
- VVC chroma quantized RD candidate scoring, reconstruction-error estimation,
  coefficient cost estimation, and shortlist policy now live in a focused
  289-line helper. It remains included in the original quantization module, so
  top-level refinement and BDPCM selection call the same private functions as
  before the split.
- VVC chroma top-level RD refinement and BDPCM candidate selection now occupy
  separate 254- and 474-line included helpers. BDPCM legality, fast-search,
  direct-SSE safety, prediction, residual construction, and winner promotion
  remain one contiguous selector called after ordinary chroma mode refinement.
- VVC SCC, temporal-hint, and finalized luma-TU metadata writes now share one
  bounded record type; temporal, exact-inter, and ordinary finalization no
  longer fan the same result out to parallel arrays through three duplicated
  write paths.
- VVC temporal-hint and finalized chroma-TU metadata writes now share the same
  bounded-record contract; inter-derived, temporal, and ordinary finalization
  explicitly record their selected mode and no longer duplicate the Cb/Cr
  result-array fan-out.
- Seven definition-only VVC residual test adapters were removed from sample
  extraction, quantization, and coefficient-stream construction; single-plane
  chroma extraction no longer carries an unreachable zero-tracking branch,
  while the live paired Cb/Cr tracked and untracked forms remain shared.
- The optional `vvc-stats` feature was made independently compilable instead of
  implicitly depending on benchmark-only internals.

All of these remain included in their original parent module scope, so the
split does not create an alternate coding path or change name resolution.

## Remaining hotspots

The next refactor should be selected from this inventory after inspecting the
current call graph rather than by line count alone:

| Area | Approximate size | Current concern |
| --- | ---: | --- |
| VVC CABAC CTU generation | 2,757 lines after mode-syntax split | tightly coupled partition traversal, neighbour state, and syntax emission |
| VVC residual prediction | 294-line orchestration before focused tests plus prediction siblings | regular luma/chroma dispatch is shared; angular and CCLM scratch ownership still need call-graph review |
| AV2 tile transform syntax helpers | 672-line core plus 360-line chroma, 320-line context, and 722-line low-level writer siblings | the low-level field/CDF writer sibling remains large and should be grouped only after syntax-by-syntax equivalence review |
| VVC residual quantization | 479-line CTU helper, 538-line mode-selection sibling, 424/493-line luma/chroma TU selection orchestration, 275/489-line search helpers, a 416-line luma mode helper, and 254/474/289-line chroma mode/BDPCM/RD helpers | selected candidates, cache transfers, materialization, and tool scoring are now separated into focused helpers; mutable chroma scratch ownership across refinement and BDPCM still needs a focused contract review |

For the next functional cleanup, prefer extracting a small shared helper with
bit-exact scalar tests over introducing a broad cross-codec abstraction. AV2
and VVC should continue sharing only stable infrastructure unless their syntax
contracts genuinely coincide.

## Validation records

The durable full-release baseline and subsequent metric records live under
`verification/generated/encode_matrix/`. Generated validation logs and
reference reconstructions are disposable; manifests, deterministic test
vectors, and recorded checkpoints are retained when intentionally needed for
regression analysis.

The current cleanup series is intentionally incremental. A future optimization
may change bytes, PSNR, or FPS, but it must report the delta against the last
recorded checkpoint and explain the heuristic or mode-selection change before
it is accepted.
