# AGENTS.md

Canonical guidance for AI coding assistants (and humans) working on the
`openDevicePartnership/INA4230` repository. Keep this file in sync with reality:
if a command, path, or convention here is wrong, fix this file in the same PR
as the underlying change.

## What this crate is

`INA4230` is a Rust crate intended to provide a driver for the Texas
Instruments INA4230 power monitor. The repository is in an **early
bootstrap state**: it was seeded from the
[`embedded-rust-template`](https://github.com/OpenDevicePartnership/embedded-rust-template)
and the scaffolding has not yet been renamed.

Concretely, as of this writing:

- `Cargo.toml` still declares `name = "embedded-rust-template"` and
  `repository = "https://github.com/OpenDevicePartnership/embedded-rust-template"`
  (see `Cargo.toml:2` and `Cargo.toml:6`). These need to be updated to
  `ina4230` / the real repo URL as part of the first substantive change.
- `src/main.rs` is the template's `Hello, world!` binary plus a `baremetal`
  module containing only a panic handler.
- There is no INA4230 driver code, no I²C register definitions, no
  `embedded-hal` integration, and no `defmt`/`log` integration yet.

Treat this AGENTS.md as the contract for *how* to add that driver code,
not as documentation of an existing driver.

## Repository layout

```
.
├── AGENTS.md                       # This file — canonical agent guidance.
├── CODEOWNERS
├── CODE_OF_CONDUCT.md
├── CONTRIBUTING.md                 # Licensing, PR etiquette, commit guidance.
├── Cargo.lock                      # Checked in; CI uses --locked everywhere.
├── Cargo.toml                      # Crate manifest; currently named embedded-rust-template.
├── LICENSE                         # MIT.
├── README.md                       # Template README (not yet rewritten for INA4230).
├── SECURITY.md
├── deny.toml                       # cargo-deny config (licenses/advisories/bans/sources).
├── rust-toolchain.toml             # Pins rustfmt + clippy components.
├── rustfmt.toml                    # max_width = 120.
├── .gitignore
├── .github/
│   ├── copilot-instructions.md     # Points at this AGENTS.md; PR-review tips.
│   ├── skills/
│   │   ├── address-review/SKILL.md # Workflow for responding to PR comments.
│   │   └── code-review/SKILL.md    # Workflow for performing PR reviews.
│   └── workflows/
│       ├── check.yml               # fmt, doc, hack-clippy, deny, test, msrv, machete.
│       ├── nostd.yml               # cargo check on thumbv8m.main-none-eabihf.
│       ├── cargo-vet.yml           # cargo vet --locked against Cargo.lock.
│       └── cargo-vet-pr-comment.yml
├── .vscode/settings.json           # rust-analyzer target = thumbv8m.main-none-eabihf.
├── src/
│   ├── main.rs                     # Template entry point + cfg-gated baremetal module.
│   └── baremetal/mod.rs            # Bare-metal panic_handler (loop {}).
└── supply-chain/                   # cargo-vet audit data.
```

There are no `tests/`, `examples/`, or `benches/` directories yet. When
adding them, mirror the conventions described below.

## Building and testing

The repository pins toolchain components via `rust-toolchain.toml`
(rustfmt + clippy) but does not pin a channel — use **stable** for everyday
work. The MSRV is **Rust 1.85** (see `Cargo.toml:7` and the `msrv` matrix in
`.github/workflows/check.yml`). The bare-metal target is
**`thumbv8m.main-none-eabihf`** (see `.github/workflows/nostd.yml` and
`.vscode/settings.json`).

Every command CI runs is reproduced below. They all currently succeed on a
clean checkout; if you change the crate, re-run the ones touching the code
you changed before pushing.

### One-time setup

```powershell
rustup target add thumbv8m.main-none-eabihf
```

For the full CI matrix you will also need:

```powershell
cargo install cargo-hack       # feature-powerset checks
cargo install cargo-deny       # license / advisory / source / ban checks
cargo install cargo-machete    # unused-dependency detection
cargo install --version 0.10.1 cargo-vet  # version pinned by .github/workflows/cargo-vet.yml
```

### Format

```powershell
cargo fmt --check
```

CI invocation: `.github/workflows/check.yml` → `fmt` job, line 37.

### Host check / build

```powershell
cargo check --locked
```

CI invocation: `.github/workflows/check.yml` → `msrv` job, line 155.

### `no_std` / embedded check

```powershell
cargo check --target thumbv8m.main-none-eabihf --locked
```

CI invocation: `.github/workflows/nostd.yml`, line 30.

### Tests

```powershell
cargo test --locked
# Full CI form (requires cargo-hack):
cargo hack --feature-powerset test --locked
```

CI invocation: `.github/workflows/check.yml` → `test` job, lines 131–132.

### Clippy

The project lints are **forbid**-level for the `suspicious`, `correctness`,
`perf`, and `style` groups (see `Cargo.toml:15-19`). CI additionally passes
`-Dwarnings` and re-asserts the same `-D clippy::*` flags on the command
line so that *any* warning, including from dependencies' generated code,
fails the build.

```powershell
# Local equivalent of CI:
cargo clippy --locked -- -Dwarnings -D clippy::suspicious -D clippy::correctness -D clippy::perf -D clippy::style

# Full CI form (feature powerset, both host + embedded targets):
cargo hack --feature-powerset --target x86_64-unknown-linux-gnu       clippy --locked -- -Dwarnings -D clippy::suspicious -D clippy::correctness -D clippy::perf -D clippy::style
cargo hack --feature-powerset --target thumbv8m.main-none-eabihf      clippy --locked -- -Dwarnings -D clippy::suspicious -D clippy::correctness -D clippy::perf -D clippy::style

# Clippy on test code:
cargo hack --feature-powerset clippy --tests --locked -- -Dwarnings -D clippy::suspicious -D clippy::correctness -D clippy::perf -D clippy::style
```

CI invocations: `.github/workflows/check.yml` → `hack-clippy` job line 99,
`test` job line 134.

### Docs

```powershell
cargo doc --no-deps --all-features
```

CI runs this on **nightly** with `RUSTDOCFLAGS=--cfg docsrs` to allow
`#[doc(cfg(...))]`-style annotations (`.github/workflows/check.yml` lines
68–71). Stable works locally; only switch to nightly if you actually use
`docsrs`-gated attributes.

### Supply-chain checks

```powershell
cargo deny --all-features --locked check
cargo machete
cargo vet --locked
```

CI invocations: `check.yml` `deny` (lines 101–118) and `machete` (lines
157–170); `cargo-vet.yml` (line 38).

### Verified-green commands

The following commands were executed at the time AGENTS.md was authored
and all completed successfully on a fresh checkout of `upstream/main`:

- `cargo fmt --check`
- `cargo check --locked`
- `cargo check --target thumbv8m.main-none-eabihf --locked`
- `cargo test --locked`
- `cargo clippy --locked -- -Dwarnings`
- `cargo doc --no-deps`

The `cargo hack`, `cargo deny`, `cargo machete`, and `cargo vet`
invocations are only exercised in CI; install the tools above before
relying on them locally.

## Code conventions

- **Formatting.** `rustfmt` with `max_width = 120` (`rustfmt.toml:1`).
  Always run `cargo fmt` before committing — CI rejects anything that
  doesn't match.
- **Edition.** `edition = "2021"` (`Cargo.toml:4`). Do not bump without a
  separate, justified change.
- **MSRV.** `rust-version = "1.85"` (`Cargo.toml:7`). Any code that
  requires a newer feature must either bump this field *and* the
  `msrv` matrix in `.github/workflows/check.yml:144` together, or be
  hidden behind a feature flag that documents the new MSRV.
- **`no_std` posture.** The crate is structured as
  `#![cfg_attr(target_os = "none", no_std)]`
  (`src/main.rs:1`). New driver code must compile under `no_std`; reserve
  `std`-using code paths for tests or `#[cfg(not(target_os = "none"))]`.
  When converting to a library, mirror the same `cfg_attr` pattern in
  `src/lib.rs` (see `README.md:46-51`).
- **Panics & indexing.** Clippy's `correctness`, `suspicious`, `perf`,
  and `style` groups are *forbidden* (not just denied). In practice this
  means: no `unwrap()`/`expect()`/`panic!()` in non-test code, no direct
  slice indexing where `.get(..)` is appropriate, no
  integer-overflow-prone arithmetic without `checked_`/`wrapping_`/
  `saturating_` as appropriate. The `address-review` skill explicitly
  treats any reviewer suggestion to use `unwrap()`/`panic!()`/raw
  indexing as **invalid**
  (`.github/skills/address-review/SKILL.md:40`).
- **Feature flags must be additive.** CI runs `cargo hack
  --feature-powerset` over every combination; enabling any subset must
  compile and pass clippy. Do not introduce mutually exclusive features.
- **Dependencies.**
  - All targets: declare under `[dependencies]`.
  - `no_std`-only deps: declare under
    `[target.'cfg(target_os = "none")'.dependencies]`
    (`Cargo.toml:12`).
  - New dependencies must be audited via `cargo vet` (CI gate) and pass
    `cargo deny` (license/advisory/source/ban). When you add a dep, run
    `cargo vet` locally and commit the resulting `supply-chain/` updates
    in the same commit.
  - Remove unused deps before pushing — `cargo machete` will fail CI.
- **Lockfile.** `Cargo.lock` is checked in (see `.gitignore` — the
  default ignore is commented out) and CI uses `--locked` everywhere.
  Commit lockfile changes deliberately; do not regenerate spuriously.

## Driver / HAL specifics

The repository does not yet contain INA4230-specific driver code. When
you add it, follow these conventions (derived from
`.github/skills/code-review/SKILL.md` and the project's clippy posture):

- Place the driver implementation in `src/lib.rs` (after converting from
  the current binary; see `README.md:32-51`). Keep `src/main.rs` only if
  there is a deliberate need for a host-side example binary.
- Express I/O over **`embedded-hal`** traits (and, where async is
  required, `embedded-hal-async`). Do not depend on a specific HAL
  crate.
- For async code, be careful with `select`/`select_array`/`select_slice`
  and similar APIs: futures that do not complete are dropped. Document
  drop-safety with a comment, and ensure no in-flight transfer state can
  be lost. The `code-review` skill calls this out explicitly
  (`.github/skills/code-review/SKILL.md:41-42`).
- Diagnostics (`defmt`, `log`) should be feature-gated. There are no
  such features today; add them as additive `defmt`/`log` features
  rather than enabling either unconditionally.
- Heap usage: none. If you need bounded collections, use `heapless`.
  Do not pull in `alloc`/`std`-only types in non-test code
  (`.github/skills/code-review/SKILL.md:45`).
- Error handling: define a crate-level `Error` enum and propagate via
  `Result`. Do not surface raw `embedded-hal` errors at the public API
  boundary; wrap them.

## Commit & PR conventions

The repository's only commit so far is `a944bdb Initial commit`, so the
conventions are inherited from `CONTRIBUTING.md` and
`.github/copilot-instructions.md` rather than from observed history.

- **Subject line.** Capitalized, ≤ 50 characters, imperative mood
  ("Add register definitions", not "Added register definitions")
  (`.github/copilot-instructions.md:13`). A short
  `type: subject` conventional-commit style (`docs:`, `feat:`, `fix:`)
  is acceptable and used by the seed `docs: add AGENTS.md` commit.
- **Body.** Separated from the subject by a blank line, wrapped at 72
  columns, explaining *what* and *why* (not *how*)
  (`.github/copilot-instructions.md:14-16`).
- **Trailers.** When AI-assisted, append an `Assisted-by:` trailer
  identifying the model, e.g.
  `Assisted-by: GitHub Copilot:claude-opus-4.7`.
- **PR etiquette** (`CONTRIBUTING.md:21-24`):
  - Open as a **draft** PR first.
  - Ensure `.github/` is present and all lint/check workflows pass on
    the draft before requesting review.
- **Licensing.** All contributions are MIT (`LICENSE`,
  `CONTRIBUTING.md:6-10`). If you contribute code authored by others or
  under a different license, call it out explicitly in the PR
  description.
- **Fixup commits during review.** Per the `address-review` skill,
  create fresh fixup commits rather than amending; the author squashes
  before merge (`.github/skills/address-review/SKILL.md:42`).
- **Regressions.** Use `git bisect` to identify the first offending
  commit when filing a regression (`CONTRIBUTING.md:28`).

## What not to do

- Do not run `cargo fmt` without `--check` in CI scripts, or commit
  formatter-modified files mixed with unrelated changes.
- Do not introduce `unwrap()`, `expect()` (outside tests), `panic!()`,
  direct slice indexing without bounds handling, or any `unsafe` block
  without a `// SAFETY:` comment. The clippy policy treats these as
  build failures.
- Do not add `std`-using code to driver paths. Anything that compiles
  for the host but not for `thumbv8m.main-none-eabihf` will break the
  `nostd.yml` job.
- Do not introduce mutually exclusive features — `cargo hack
  --feature-powerset` will fail.
- Do not add a dependency without running `cargo vet` and committing
  the audit updates; CI's `cargo-vet` gate will block the PR otherwise.
- Do not remove `Cargo.lock` or commit unrelated lockfile churn.
- Do not edit `supply-chain/` by hand — let `cargo vet` manage it.
- Do not amend or force-push to a PR branch during review; use fixup
  commits (`.github/skills/address-review/SKILL.md:42`).
- Do not change global git config when committing on behalf of another
  author; pass `-c user.name=... -c user.email=...` per commit.

## How to find more context

- `README.md` — current contents are still the template's
  "Customizing This Template" guide; useful for the binary→library
  conversion checklist (`README.md:32-63`).
- `CONTRIBUTING.md` — licensing, PR etiquette, regression workflow.
- `.github/copilot-instructions.md` — short pointer to this file plus
  PR-review focus areas.
- `.github/skills/code-review/SKILL.md` — what an AI code review on
  this repo should and should not flag.
- `.github/skills/address-review/SKILL.md` — how to respond to PR
  review comments (including which suggestions to push back on).
- `.github/workflows/check.yml`, `nostd.yml`, `cargo-vet.yml` —
  ground truth for the exact commands CI runs and the toolchain matrix.
- `deny.toml` — the cargo-deny policy this crate is audited against.
- `supply-chain/` — `cargo vet` audit state; inspect via
  `cargo vet inspect` rather than by reading files directly.
- The TI **INA4230** datasheet (TI literature number `SBOSAH9` or
  successor) — authoritative reference for any register, scaling, or
  protocol behaviour added to the driver.

## Incorporated from `.github/copilot-instructions.md`

The following is the full, verbatim content of
`.github/copilot-instructions.md` as of this commit, included here so
this AGENTS.md remains a strict superset:

> # Rust PR Review Instructions
> CI overview:
> * CI will build the project and run `cargo test` and `cargo clippy`.
> * Feature combinations are checked with `cargo hack`.
> * Do not comment on compile errors, compiler warnings, or clippy warnings.
>
> Pay special attention to...
> * code that uses async selection APIs such as `select`, `selectN`,
>   `select_array`, `select_slice`, or is marked with a drop safety
>   comment. These functions drop the futures that don't finish. Check
>   that values are not lost when this happens.
> * code that could possibly panic or is marked with a panic safety
>   comment.
>
> # Commit Messages
>
> - Subject line: capitalized, 50 characters or less, imperative mood
>   (e.g., "Fix bug" not "Fixed bug")
> - Separate subject from body with a blank line
> - Wrap body text at 72 characters
> - Use the body to explain *what* and *why*, not *how*
