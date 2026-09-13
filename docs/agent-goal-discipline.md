# Agent goal discipline

This document turns the repository goals in `AGENTS.md` into a concrete
development process for coding agents. It exists because optimization work can
create local pressure to solve the case in front of the agent while drifting
from the broader project goals: shared codec paths, strict validation, small
public contracts, and reproducible evidence.

Use this document for nontrivial implementation, optimization, validation,
CLI/API, mode-search, feature-gate, and generated-artifact changes.

## Internal pull request cycle

Before editing, write a short preflight note:

```text
Goal:
Shared path affected:
Policy or mode hook being changed:
Potential split-path risk:
Broader contracts at risk:
Existing foundations, prior attempts and docs checked:
Research applicability or reason not needed:
Smallest next step, disproof and stop condition:
Validation planned:
Full six-stream evidence and neighboring contract map:
```

If the shared path cannot be named, inspect more before changing code. If a new
split path appears necessary, record the codec-specification or
reference-validation boundary before implementing it.

During implementation:

- Prefer policy data, mode availability, candidate scoring, syntax parameters,
  or small leaf primitives inside the shared traversal/reconstruction pipeline.
- Do not create a second lossless/lossy, intra/inter, screen/camera, RGB/YUV,
  chroma/profile, traversal, residual, reconstruction, entropy, validation, or
  public-contract path unless the boundary has already been justified.
- Do not remove, ignore, downgrade, or loosen a failing test, validation check,
  lint, assertion, or manifest rule to make progress. Replace obsolete checks
  with more accurate checks in the same change.
- Keep experiments default-off until correctness, quality, speed, and product
  scope are proven for the intended default.

After the diff, review it as an internal pull request before committing or
reporting completion:

```text
Architecture outcome:
Shared path and working foundation preserved or changed:
Retained split paths and justification:
Tests changed and contract strength:
Validation and metrics:
Per-row parent/fixed-baseline evidence, uncertainty and unresolved holds:
Generated artifacts created:
Public API/CLI/settings/docs impact:
Remaining risk:
Outcome and next evidence:
```

The review must check for copied loops, broad defaults changed for one case,
weakened gates, hidden dead code, stale docs/help, relaxed validation, removed
tests, and behavior changes outside the stated goal.

Every change is subject to the
[mandatory six-stream gate](validation.md#mandatory-six-stream-change-gate).
Bind the review to the actual diff/revision, binary, settings and inputs. Reuse
earlier measurements only with demonstrated unchanged artifacts and applicability;
a prior PASS label does not establish this. Map feature preservation, compression/
quality, FPS/resources, streaming/lifecycle, neighboring modes/frontends, shared
safe architecture, validation integrity, documentation and portability to named
evidence. Missing, stale or contradictory evidence stays pending. Reload the
current contract and owner amendments after compaction/resume/project switches
and before checkpoint or commit decisions; summaries are navigation aids.

An individual working fix may be committed after an issue-specific review of
its functional evidence and the tractability of remaining performance costs.
This is not a blanket fix-now/optimize-later policy or a regression waiver. Record
why retaining the fix is justified, the open losses, the next performance step,
and when to revert or replace the approach in a new reviewed commit. Functional
acceptance, commit authorization and full checkpoint/performance acceptance are
separate decisions; the final gates above remain required.

Carry stable unresolved issue IDs and the full neighboring-contract map into
each review and append-only history record. Bind the actual tree, parent and
applicable binary/evidence; compare both the original fixed baseline and immediate
parent. A slower parent, narrower passing test or favorable average cannot close
an older failure. Close an issue only with the resolving revision and comparable
evidence, preserving the original failure and rejected attempts.

### Research and staged improvements

Apply this process to nontrivial algorithm, compression, performance and memory
improvements through the existing preflight and completion fields. It adds no
parallel approval system. An obvious local repair needs proportionate source and
evidence reasoning, not a compulsory paper search. Research and experiments stay
within the authorized scope and independent review requirements.

Name the measured problem, affected shared path, workload, baseline revisions,
constraints and intended benefit. Separate observations from hypotheses. For
memory, distinguish live ownership, requested heap, allocator residency, process
RSS and whole-host memory; one does not establish another. Whole-stream buffering
is not evidence of compliant streaming. Preserve valid historical
byte/quality/performance measurements with their capability and resource
qualifications; it does not waive the fixed-baseline comparisons. Invalid
bitstreams remain invalid quality baselines under the existing gate; preserve
their failure evidence.

Inventory working primitives, callers, tests, instrumentation, reference tools
and prior attempts before designing a replacement. Reuse the shared foundation;
explain what new evidence makes a rejected idea worth reconsidering. Keep working
production foundations and portable regression tests versioned in product Git.
Preserve rejected patches, empirical measurements and external research/comparator
material outside product Git. Do not retain dead experimental production paths
or erase useful tests. Replacing a foundation requires evidence that its
contracts survive.

Both the coding worker and supervisor may initiate focused online research and
upstream-code inspection when evidence or understanding is missing. The worker
does not wait for all ideas from the supervisor or a later roadmap phase. Use
primary sources and follow relevant foundations, assumptions, later corrections
and limitations. Pin source versions and record sections/functions actually read,
unavailable sources, discrepancies, provenance and license boundaries. Bound the
search by a concrete question and the next discriminating experiment; stop when
that evidence is sufficient. External claims need local applicability checks.
Research initiative authorizes neither another actor or heavy job nor bypassing
independent experiment review.

Distinguish previous-FFE product baselines from a technique's simpler algorithmic
baseline. Reuse a validated foundation or establish the smallest applicable
method before advanced extensions. Any new simple scalar oracle is
validation-only, never a second production path. Map equations, state, numerical
rules and boundary assumptions to the shared implementation; check independent
examples and required exactness or tolerances. Do not reproduce an entire upstream
system for one primitive. Record when existing validated foundations already
satisfy this stage. A local lifetime fix needs no artificial paper algorithm.

Separate reproduction, adaptation and optimization into minimal independently
evaluated steps. Each needs a predicted effect, a falsifying observation and a
stop condition before execution. Preserve the shared implementation and normative
codec syntax/reconstruction. Check extensions separately after their prerequisites;
a small interaction experiment may combine steps when its mechanism is explicit.
Avoid changing algorithm, coding policy, data representation and lifetime together
without that justification.

Reuse useful profiles first. Choose the smallest diagnostic that distinguishes
hypotheses and record its overhead, timing boundaries and resource effects.
Diagnostic cost is not ordinary performance: judge with ordinary release runs.
Focused failures can reject a candidate early; surviving production changes
still face the existing gates. Do not repeat expensive unchanged runs for
paperwork or collect evidence indefinitely instead of making a bounded decision.

Review actual diffs, commands and evidence. Compare the immediate parent and
original fixed baseline, plus the algorithmic baseline where applicable. Retain
adverse rows, unknowns and stable issue IDs; neither a favorable average nor a
slower parent erases a loss. Record accepted, rejected or inconclusive scope,
the foundation preserved and next evidence needed. Agreement between agents is
not an oracle. Existing acceptance requirements remain: no performance waiver
or reset of cumulative debt.

## Tablecloth check

Before finalizing a narrow fix, identify the surrounding contracts that share
the changed code. At minimum, consider:

- lossless and lossy;
- intra, inter, and predictive modes;
- screen content and camera-like content;
- RGB, YUV, subsampled formats, and high bit depth;
- luma, chroma, and component-specific syntax;
- source filters, file input, CLI, API, and WASM callers;
- smoke, regression, multi-CTU, and representative matrix rows;
- generated artifacts, ignored local fixtures, and committed fixtures.

The change should not fix one row by pulling behavior out from under another.
If a neighboring mode is intentionally left unsupported or gated off, state why.

## Known drift patterns

These are deviations the project has already encountered. Treat them as warning
signs during preflight and review.

- Reference-clean but product-bad changes: a stream can decode correctly while
  bytes, PSNR, or FPS regress. Required reference validation is necessary but
  not sufficient for full checkpoint acceptance. Every change needs full six-stream
  per-row evidence and neighboring correctness, including correctness repairs
  and streaming refactors; short synthetic probes cannot replace that gate.
- Broad gates from narrow wins: a policy that helps one format, bit depth, or
  content class can damage adjacent ones. Start new gates as explicit allow
  lists with neighboring counterexamples checked.
- Local proxy metrics pretending to be full decisions: payload length,
  coefficient-local SSE, raw prediction SSE, or proxy mode ranking must be
  calibrated against whole-frame bytes, reconstruction quality, and state.
- Timing-noise optimizations: byte-identical changes still need matched
  baseline/current runs, a minimum effect size, and profiler evidence before
  they are retained.
- Experimental scaffolding leaking into production: syntax helpers, mode
  counters, or partially wired tools must remain default-off until the complete
  production path passes required reference validation and representative
  metrics.
- Average-score masking: an aggregate win cannot excuse a harmed row or mode.
  A minor necessary cost is eligible only for a major repair with a measured
  cause/benefit and compensating companion improvements, reviewed together.
  Substantial or unexplained losses block final performance acceptance; no automatic
  percentage allowance or credit for a promise of future optimization.
- Feature loss disguised as a fix: disabling prediction/SCC, forcing intra,
  changing QP/defaults or narrowing formats/geometry/length can produce a valid
  stream while abandoning the product goal. Verify emitted prediction and
  reference behavior on changed frames, repeated frames and GOP resets. Labels
  and decoder success alone do not prove block prediction or compression benefit.
  Use portable generated-data regressions that fail under all-intra substitution,
  with exact lossless or appropriate full-sequence lossy quality controls.
- Stale public contracts: CLI/API/settings changes must update parser behavior,
  manifest/config ownership, docs, help text, and contract tests together.
- Goals rewritten to fit implementation: official product documentation is
  normative. Repair divergent code rather than weakening the documented goal.
  Keep signatures and examples accurate and runnable; identify unfinished
  behavior, including VVC/WASM streaming work, as implementation gaps rather
  than compliant exceptions. Make intended contract changes explicit.
- Recurring whole-stream buffering: a per-frame facade can still retain every
  input, recreate the encoder on each frame, retain transmitted chunks, or
  silently aggregate history in examples. Review against the
  [API streaming contract](api-v0.md#encoder-sessions): verify observable output
  before finalization, persistent codec state, and bounded internal retention
  independent of stream duration. Current no-reordering modes must emit output
  per frame. Future B-frame/lookahead delay and storage require explicit bounds;
  whole-stream retention is permitted only as explicit caller-owned recording.
  Primary streaming examples must consume/drain each step before accepting more
  input. Each relevant fix must add portable generated-data behavioral tests
  for these properties, source/session parity and lifecycle behavior, with
  required-reference multi-frame coverage for affected codec modes. Cite the
  actual test locations and results as they land; do not claim pending tests
  or unfinished frontends are covered.
- Validation manifest drift: skipped rows, local manifests, renamed columns, or
  generated-vector changes are not neutral. Explain skips and keep strict claims
  on required validation.
- Retrying rejected probes: search `docs/compiler-optimizations.md` before
  mode-search, CCLM, BDPCM, MTS, InterSkip, IBC, motion, or transform probes.
  Do not repeat a rejected idea without a new hypothesis and a stronger gate.

## Future drift patterns

These have not necessarily happened yet, but they are plausible failure modes
for this codebase.

- Benchmark overfitting: do not tune only for smoke tests, one screen-content
  vector, or the currently visible matrix. Use holdout or less-familiar rows
  before accepting broad heuristics.
- Baseline drift: every metric claim should name the baseline commit, current
  commit or diff state, commands, vector set, reference mode, and whether the
  binary was rebuilt.
- Reference-tool drift: record the reference decoder path/version and relevant
  flags when making compatibility claims.
- Optimization by new knobs: avoid adding public or semi-public settings in
  place of better automatic mode policy. New settings need manifest entries,
  legality checks, docs, tests, defaults, and a product rationale.
- Feature-gate rot: code behind optional features must keep compiling and must
  not silently diverge from product behavior. Run feature and dead-code audits
  after changing conditional compilation.
- Test narrowing: changing a test can weaken a contract as much as deleting it.
  State whether each changed test became stronger, weaker, or different.
- Brittle golden tests: distinguish contract tests, reference compatibility
  tests, and intentionally brittle byte fixtures so correct refactors are not
  blocked or papered over.
- State-lifetime mistakes: scratch, cache, entropy, CTU, slice, frame, and
  reconstruction state changes need explicit lifetime ownership and tests that
  cross the affected boundary.
- Partial profile support becoming implied support: syntax support does not mean
  a full tool, profile, or product path is enabled. Keep explicit
  "implemented but disabled" markers until validation proves default support.
- Artifact contamination: final notes must separate committed source changes,
  ignored generated files, local fixtures, benchmark reports, and validation
  outputs.
- Public-contract side effects: codec-local fixes can change CLI defaults, API
  setting legality, WASM behavior, docs, examples, or release claims. Include
  public surfaces in the tablecloth check.
- Over-unification: avoiding duplicate paths must not hide real
  specification-level syntax or reconstruction differences. Shared abstractions
  need the same proof as split paths: they must preserve visible codec
  boundaries.
- Documentation laundering: do not turn a failing behavior into a documented
  limitation unless it is intentionally out of scope, gated safely, and covered
  by validation or contract tests.
- Dependency and license creep: do not add dependencies, imported code, or
  generated assets for convenience without checking scope, license, build
  impact, and whether local code already provides the needed primitive.
- Error-surface drift: unsupported user input must remain rejected before
  bitstream emission. Do not replace typed errors with panics, silent fallback,
  or lossy coercion for external inputs.

## Optional independent audit

When a workflow explicitly allows a second agent or reviewer, use it for a
narrow architecture review rather than another optimization pass. The audit
brief should be:

```text
Review this diff against AGENTS.md and docs/agent-goal-discipline.md.
Focus on duplicated paths, weakened validation, broad gates, public-contract
side effects, missing tablecloth coverage, and unjustified retained risk.
```

The implementing agent remains responsible for the final decision, validation,
and documentation.
