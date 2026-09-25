<!-- Write all PR text in Simplified Technical English: short sentences, one idea per sentence, simple words, active voice. See CONTRIBUTING.md. -->

## Thinking Path

<!--
  Required. Go from what the project is down to this change, in 4–7 steps.
  See CONTRIBUTING.md for examples.
-->

> - space-view builds disktree, a Windows treemap for finding and removing what fills a disk.
> - [Which part of the app is involved]
> - [What problem or gap exists]
> - This PR ...
> - The benefit is ...

## Linked Issues or Issue Description

<!--
  Required. Either:
  (A) link public issues/PRs: `Fixes #12`, `Closes #12`, `Refs #12` (include duplicates and prior PRs), or
  (B) describe the problem here.
      Bug: what happened, expected, steps to reproduce, disktree version/commit, Windows version.
      Feature: problem, proposed solution, alternatives considered.
  Bigger or UX-direction changes need an agreed issue first.
  No internal ticket ids, private tracker links or localhost URLs.
-->

-

## What Changed

<!--
  One bullet per logical change.
  Call out explicitly if you touched: build.rs, src/icon_design.rs, Cargo.toml/Cargo.lock
  dependencies, .github/workflows, scripts, .cargo/config.toml, rust-toolchain.toml.
-->

-

## Verification

<!--
  Paste the output. First-time fork contributors' CI waits for maintainer approval, so this is the evidence.
  Required: cargo fmt --check, cargo clippy --all-targets -- -D warnings, cargo test --lib.
  Windows-only code: Windows `cargo test --lib`, or cargo-xwin + wine (see CONTRIBUTING.md).
  UI changes: before/after screenshots of the same folder, same window size, same selection.
-->

```
$ cargo fmt --check
$ cargo clippy --all-targets -- -D warnings
$ cargo test --lib
```

## Risks

<!--
  What could go wrong? Required detail if you touched delete, safety rules, dialogs, the scanner,
  or build-time code. Write "Low risk" only if it really is.
-->

-

## Model Used

<!--
  Required. Which AI model produced or assisted this change?
  Write "not exposed" for a detail you cannot see. Do not guess.
  If no AI was used, write: None, human-authored
  AI agents: also state in this section that the PR was written by an AI agent.
-->

- **Provider:**
- **Exact model ID / version:**
- **Context window:**
- **Reasoning / effort mode:**
- **Tool / agent harness:** <!-- e.g. Cursor cloud agent, Cursor IDE, Claude Code, Codex CLI, Copilot -->
- **Written by an AI agent?** <!-- yes / assisted / no -->

## Checklist

- [ ] I searched open and closed issues and PRs for duplicates and linked related ones above
- [ ] This PR is one logical change, and bigger or UX-direction changes were agreed in an issue first
- [ ] The Thinking Path goes from the project down to this change
- [ ] Model Used is complete (provider, exact model ID, context window, effort mode, harness), or says "None, human-authored"
- [ ] I pasted local output of `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --lib`
- [ ] I tested Windows-only code on Windows or with cargo-xwin + wine (or this PR has none)
- [ ] UI changes have before/after screenshots of the same folder, keep every file distinguishable, and keep the colourful direction (or this PR has no UI change)
- [ ] Delete safety is unchanged: Recycle Bin default, two confirmations for permanent delete, protected paths, "permanent" in labels
- [ ] Delete/safety/scanner changes have new tests and a Risks note (or this PR has none)
- [ ] I called out any change to build.rs, src/icon_design.rs, dependencies, CI, or scripts (or this PR has none)
- [ ] No binaries or build output are committed
- [ ] Only public issue/PR references, and the branch name is descriptive kebab-case
- [ ] I credited the original author if this builds on someone else's PR
