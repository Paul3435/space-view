# Instructions for AI coding agents

Read and follow [CONTRIBUTING.md](CONTRIBUTING.md). It applies to agents exactly as it does to
humans. The short version:

- **Say you are an AI.** State it in the PR body and fill in **Model Used** in
  [the PR template](.github/PULL_REQUEST_TEMPLATE.md): provider, exact model ID, context window,
  reasoning/effort mode, and harness (e.g. Cursor cloud agent, Claude Code, Codex). Write "not
  exposed" for anything you cannot see; do not guess. Add an `Assisted-by:` trailer to commits.
- **Search first** for existing issues and PRs. Help an in-flight PR instead of duplicating it, and
  credit its author.
- **Discuss big or visual-direction changes in an issue first.** Keep the treemap colourful and
  every file distinguishable.
- **Run and paste** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and
  `cargo test --lib`. Test Windows-only code with cargo-xwin + wine (see CONTRIBUTING.md). Do not
  open a PR you could not test.
- **Never weaken delete safety** (`src/ops.rs`, `src/safety.rs`, `src/app/dialogs.rs`): Recycle Bin
  default, two confirmations for permanent delete, protected paths, "permanent" in labels.
- **Call out build-time code**: `build.rs`, `src/icon_design.rs` (included by `build.rs`),
  dependencies, CI workflows, scripts.
- One logical change per PR. No committed binaries. Only public issue links. Descriptive kebab-case
  branch names.

See [README.md](README.md) for the project layout and build instructions.
