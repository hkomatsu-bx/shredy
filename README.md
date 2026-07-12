# Shredy

English | [日本語](README.ja.md)

A portable, Windows-only tool for organizing duplicate and similar files. It detects duplicate or similar files across multiple folders and lets you delete, move, and organize them safely. **Fully implemented in Rust, with two frontends (GUI via Slint and CLI) sharing a common `shredy-core` engine.**

- **Version:** 1.0.3
- **Target:** Windows 11 (build 26100 or later, x64) only
- **License:** GPL-3.0-only
- **Language:** Rust (edition 2021 / rust 1.97+)

> It depends on the Windows API for the Recycle Bin, `\\?\` long paths, reparse-point detection, `explorer.exe` integration, and more, so it **cannot be built or run on other operating systems**.

---

## Main use cases

- Deduplicating files with identical content (byte-for-byte comparison)
- Deleting backup copies and stale old files
- Grouping and organizing similar images (pHash)
- Detecting content-identical Office files while ignoring metadata differences
- Cleaning up empty, temporary, and corrupted files
- Comparing folder differences and batch-renaming large numbers of files

## Design principles

- Portable operation without using the registry (xcopy deployment)
- Safety-first design to prevent accidental deletion (Recycle Bin deletion by default; at least one file is always kept per duplicate group)
- Full support for Unicode (NFC normalization) and long paths (`\\?\`, up to 4096 chars)
- Fast operation in Rust, with automatic I/O parallelism tuning that accounts for NAS (UNC)

---

## Architecture

Two frontends (CLI / GUI) depend unidirectionally on the shared `shredy-core` engine, while Windows-specific processing is isolated in the lowest layer, `shredy-sys`.

```
┌─────────────┐   ┌──────────────────────────────┐
│  shredy-cli │   │          shredy-gui           │
│ (shredy.exe)│   │ (shredy-gui.exe + lib)        │
│ search+list │   │ UI(Slint)/bridge/wiring/state │
└──────┬──────┘   └───────────────┬──────────────┘
       │  both depend on core one-way   │
       └────────────┬────────────────┘
                    ▼
         ┌────────────────────┐
         │     shredy-core      │  scan / compare strategies /
         │  (core logic, pure   │  select / delete / config /
         │        Rust)         │  utilities / logging
         └──────────┬─────────┘
                    ▼
         ┌────────────────────┐
         │     shredy-sys       │  reparse detection / in-use
         │ (low-level Win API)  │  detection / timezone lookup
         └────────────────────┘
```

| Crate | Role | Output |
|---|---|---|
| `shredy-sys` | Low-level Windows API wrappers (reparse / in-use / timezone) | lib |
| `shredy-core` | Search engine, compare strategies, selection, delete planning/execution, config, cache, utilities, rename, logging | lib |
| `shredy-cli` | Argument parsing → search → formatted result output | `shredy.exe` |
| `shredy-gui` | UI definition (Slint), type conversion, event wiring, state management | `shredy-gui.exe` + lib |

Destructive operations — deletion, moving, generating manual-delete lists, quarantining corrupted files — are **handled by the GUI and are not available in the CLI** (separation of concerns; safety first).

---

## Build, test, and run

A Cargo workspace. **Windows is required.**

```bash
cargo build [--release]                                  # release uses windows_subsystem=windows + a size-optimized profile
cargo test --workspace                                   # narrow with -p <crate> / <name>
cargo clippy --workspace --all-targets -- -D warnings    # treat warnings as errors
cargo fmt --all                                          # verify with --check
cargo bench -p shredy-core                               # criterion benchmarks
cargo deny check                                         # license / advisory audit
```

The release build is size-optimized (`opt-level="s"` / `lto=fat` / `codegen-units=1` / `strip`). `panic="abort"` is **not** used, so that the safety mechanism catching the internal panic from parsing malformed `.xls` files via `catch_unwind` remains effective.

---

## CLI usage

`shredy.exe` is **specialized for duplicate-file search and result listing**.

```
shredy.exe [options...] [folder paths...]
```

If folder paths are omitted, the selected folders in the `TargetFolders` setting are used.

| Option | Description |
|---|---|
| `--config <path>` | Path to the config file (`shredy.config`) — a folder or file path |
| `-S[0-8]` | Compare method (see below). Defaults to the `CompareMethod` setting if omitted |
| `--io-parallelism <N>` | I/O parallelism (overrides automatic throttling on NAS) |
| `--version` | Print the version string |

### Compare methods (`-S`)

| Value | Method |
|---|---|
| 0 | BLAKE3 hash |
| 1 | SHA-256 hash |
| 2 | XXH3-128 hash |
| 3 | Partial match (ranged XXH3) |
| 4 | Name + size + date |
| 5 | Name + size |
| 6 | Name only |
| 7 | Office similarity (hash of normalized content) |
| 8 | Image similarity (pHash → BK-tree → Union-Find) |

```bat
REM Search the given folders with BLAKE3 and list the results
shredy.exe -S0 "D:\Photos" "D:\Photos_Backup"
```

---

## Safety design (cross-cutting constraints)

| Constraint | Details |
|---|---|
| Safety first | Deletion goes to the Recycle Bin by default. Directory reparse points (symbolic links / junctions) are skipped to avoid pulling in their targets. Locked / in-use files are excluded |
| "Keep one" | At least one file is kept per duplicate group (the keep rule follows one of 6 `KeepFileRule` variants) |
| Long paths / Unicode | `\\?\` (up to 4096 chars) / NFC normalization + case-insensitive comparison |
| Responsive cancellation | Immediate interruption even during scanning/comparison (recursion depth capped at 64 to prevent loops) |
| NAS-aware | If most targets are UNC paths, I/O parallelism is automatically limited to 8 (an explicit `--io-parallelism` is respected) |
| Error continuation | Inaccessible files are individually skipped and processing continues; only fatal errors are reported |

---

## Documentation

The source code (`crates/*`) is the authoritative record of what is actually implemented; when documentation and code disagree, the code takes precedence.

The design and requirements documents produced during development (AI-DLC artifacts — functional spec, acceptance criteria, non-functional requirements, and as-built architecture records) have completed their role and are archived on the `aidlc` branch, kept separate from `main`.

---

## License

GPL-3.0-only. Because the GUI's Slint (GPL-3.0 or commercial) and the image HEIC decoder (AGPL-3.0 or commercial) are used under their GPL/AGPL options, the tool itself is distributed under GPL-3.0. See `LICENSE` / `THIRD-PARTY-LICENSES.md` for details.
