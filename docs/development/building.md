# Rust builds and CI

## Build profiles

Three profiles (`crates/Cargo.toml`) with separate jobs; the local development loop never builds `release`.

| Profile | Runs | Own code | Dependencies |
|---|---|---|---|
| `dev` | `just cargo-test`, `just cargo-lint`, prek hooks (pre-commit clippy, pre-push) | opt-level 0, full debug info | opt-level 0 |
| `dev-runtime` | `just viewer`, `just game` | opt-level 1, `debug = "line-tables-only"` | opt-level 3, build scripts and proc macros too (`build-override`) |
| `release` | `just viewer-release`, `just game-release`, `just check-maps`, manual benchmarks, release artifacts | opt-level 3 | opt-level 3 |

`dev-runtime` exists because Bevy is only usable at dependency opt-level 3, while the edit-rebuild loop needs own code to compile fast. It was measured on this machine (Windows, rustc 1.99, warm cache, one-line edit; the first measurement of a case is dropped when the first run carries a one-off cost):

| Warm loop | `release` (before) | `dev-runtime` static | + rust-lld | + bevy_dylib |
|---|---|---|---|---|
| edit in `kn-render` → viewer binary | 7.6 s | 6.8 s | 5.9 s | **2.2 s** |
| edit in the viewer binary → relink | 4.2 s | 7.5 s | 4.5 s | **1.0 s** |
| no-op | 0.3 s | 0.3 s | 0.4 s | 0.4 s |

Three levers, each measured:

- **rust-lld** (`.cargo/config.toml`, `[target.x86_64-pc-windows-msvc] linker = "rust-lld"`,
  `-C linker-flavor=lld-link`). Bundled with the toolchain (rustc ≥ 1.70), nothing to install;
  CI is ubuntu-latest and the section does not touch it. Applies to every local Windows
  build, including dev test binaries. Adding rustflags rebuilds every cached unit once
  (~2.5 min per profile); delete the file to fall back to `link.exe`.
- **`debug = "line-tables-only"`** for `dev-runtime`: PDB writing is the Windows link cost,
  backtraces keep file and line. Not measured against `debug = 2` separately — with the two
  levers below the relink is 1 s anyway.
- **`bevy/dynamic_linking`** behind the `dynamic` feature of `kn-fe-viewer`/`kn-fe-player`,
  passed by the `viewer`/`game` recipes. Bevy moves into `bevy_dylib.dll` (137 MB in
  `crates/target/dev-runtime/deps`), the app binary links in ~1 s. Both windows verified on
  Windows/Vulkan. Caveats: the exe only starts through `cargo run`/`just` (the DLL is found
  via the `deps` directory on the PATH cargo sets); launching it directly needs the DLL
  copied next to it. Own code must stay at opt-level ≤ 1 — true for `dev-runtime`. The
  feature is opt-in per recipe: drop `--features …/dynamic` to link statically.

`just check-maps` intentionally remains on `release`: it is headless parsing, fully CPU-bound
in own code (no Bevy rendering), and runs an order of magnitude slower in `dev-runtime`
(measured 77 s for all 50 RUS maps; ~5 s in release). This exception is the only place where
the release profile is still needed locally — every viewer/player recipe switched to dev-runtime.

Bevy features are declared once in `[workspace.dependencies]` with `default-features = false`
and only what both apps use: 2D sprites, Feathers UI, text with the embedded default font,
picking, winit. The default 3D/PBR/glTF/audio/gizmos/anti-alias/post-process chain is off
(no code references to it — grep over `crates/`); what remains of `bevy_gizmos`, `bevy_gilrs`,
`bevy_scene`, `bevy_animation`, `bevy_state` comes in through `bevy_feathers` implications.
Both windows verified after the cut; the regression found during the audit was the missing
`bevy_ui_render`/`ui_bevy_render` (the Feathers panel panics with `DrawFunctions<TransparentUi>
must be added` without them). The feature set lives in one place now, and cargo unifies it
across all five Bevy crates, so an addition in one crate cannot silently re-enable defaults
for the others.

sccache is optional, not part of the setup: `RUSTC_WRAPPER=sccache` (after installing it
separately) shares compiled dependencies between the agent worktrees, each of which keeps its
own `crates/target` (the cache is per machine, the target directories stay separate). Without
it a fresh worktree pays one cold dependency build per profile (≈2.5–3 min each here).

## CI (`.github/workflows/ci.yml`)

Measured on GitHub-hosted runners, rustc 1.99.1, 2026-11-13, each row its own run
(PR #120 runs 43404019876 cold, 43407464572 warm):

| Job | Runner | Cold | Warm |
|---|---|---|---|
| `rust` — fmt, clippy, test (`dev`) | ubuntu-latest | 7m32s | 3m56s |
| `build` — `kn-viewer` + `kn-player`, full link (`dev-runtime`) | ubuntu-latest | 14m50s | 1m39s |
| `build-windows` — same as `build` | windows-latest | 21m1s | 1m44s |

The `build` job is the PR gate; `build-windows` runs on push to main and `workflow_dispatch` only.
The same profile measures 1.4× slower on windows-latest than on ubuntu-latest (`cargo build
--timings`: 503 units, 3379 s of rustc in 859 s of wall on Linux against 4635 s in 1204 s on
Windows), so the gate lives on Linux. A Windows-only regression is caught by the post-merge smoke
on main, not on the PR — an accepted trade-off; the release path still requires a green main run
(`version.yml`). Cold build profile is identical on both runners: `bevy_*` crates take 56–58% of
the rustc time (`bevy_feathers` 141 s, `bevy_render` 137 s, `bevy_ui` and `bevy_sprite_render`
121 s each on Linux), so the job time hinges on dependency compilation, not on own code
(`kn-render` compiles in 39 s at opt-level 1).

Dependency compilation is cached with `Swatinem/rust-cache@v2` (`workspaces: crates`): only
dependencies are cached, workspace crates rebuild every run by design; the job id is part of the
cache key by default, so the three jobs keep separate caches without extra inputs — measured
≈1.1 GB (`dev`), ≈0.8 GB (`dev-runtime` Linux), ≈0.7 GB (`dev-runtime` Windows), restore 23–34 s
per job, and no re-save on an exact key hit. Both build jobs run `--timings` and upload
`cargo-timing.html` as a run artifact, so every regression in build time is attributable per
crate. There is no nightly schedule: pushes to main happen several times a day and every green
main run refreshes the caches; revisit if the cadence drops (GitHub evicts caches after 7 days
of disuse, and the repo-wide limit is 10 GB).
