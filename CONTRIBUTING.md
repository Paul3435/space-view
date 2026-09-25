# Contributing to space-view

Thanks for wanting to help! space-view builds **disktree**, a small native Windows treemap for
finding and safely removing what fills a disk (Rust + egui). Small fixes and thoughtful larger
changes are both welcome.

This guide is short on purpose. The rules that matter most are **delete safety**, **Model
Used**, and **showing your test output**.

## Before you start: search first

Search the [issues](https://github.com/hawikk/space-view/issues) and
[pull requests](https://github.com/hawikk/space-view/pulls) (open *and* closed) for the same area.

- **Duplicate or in-flight PR?** Help it over the line instead of opening a parallel one (see
  [Helping other contributors](#helping-other-contributors)).
- **Related issues?** Link all of them in your PR.
- **Old PR is dead** (stale, or painful to rebase)? A fresh PR is fine. Link the old one and
  credit its author.

A one-minute search saves a reviewer an hour.

## Two paths to a merged PR

### Path 1: small, focused changes (fastest)

- Fix or improve **one** thing.
- Touch as few files as you can.
- Pass all [checks](#checks-every-pr) and fill in the [PR template](.github/PULL_REQUEST_TEMPLATE.md).

Bug fixes, docs, tests and small UI polish almost always land quickly when they are clean.

### Path 2: bigger or UX-direction changes (discuss first)

Open a [GitHub issue](https://github.com/hawikk/space-view/issues/new) **before** you write code if
your change:

- changes how the treemap or the app *looks* overall (palette, tile style, layout, chrome),
- changes delete behaviour, the scanner, or what the numbers mean,
- adds a dependency, a new feature, or a new screen.

Describe the problem, your idea, and a mock-up or screenshot if it is visual. Build it once there is
rough agreement. Unannounced redesigns may be closed even when the work is good: they often go in a
different direction than the project (a recent PR muted the treemap while the owner was making it
*more* colourful).

## Checks (every PR)

Run these locally. All three must pass:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
```

Much of the code is Windows-only (`cfg(windows)`), so also build and test the Windows target when
you touch `ops.rs`, `fsread.rs`, `scan.rs`, `main.rs`, `crash.rs`, `diag.rs` or anything behind
`cfg(windows)`. On Windows, `cargo test --lib` covers it. From Linux or macOS (see the
[README](README.md#building)):

```bash
rustup target add x86_64-pc-windows-msvc
cargo install cargo-xwin
sudo apt install llvm clang lld
./scripts/build-windows.sh                    # -> dist/disktree.exe
cargo xwin test --lib --no-run --release --target x86_64-pc-windows-msvc
wine target/x86_64-pc-windows-msvc/release/deps/disktree-*.exe
cargo xwin clippy --all-targets --target x86_64-pc-windows-msvc   # no *new* warnings
```

**Paste the output** (at least the `test result:` lines and any warnings) into the Verification
section of your PR. GitHub does not run CI for a first-time fork contributor until a maintainer
approves it, so until then your local output is the only evidence the change works. "I could not
run the tests" means the PR is not ready yet.

## Project rules

### Delete safety is non-negotiable

disktree deletes people's files. These must never get weaker:

- **Recycle Bin is the default.** Delete goes through the shell with
  `FOFX_RECYCLEONDELETE | FOF_ALLOWUNDO | FOF_WANTNUKEWARNING` (`src/ops.rs`).
- **Permanent delete needs two confirmations**, with the delayed final button and Cancel in the
  old button's spot (`src/app/dialogs.rs`).
- **The protected-path rules** in `src/safety.rs` (drive/share roots, Windows, Program Files,
  ProgramData, Users, profiles, system items, links) and the typed-name confirmation.
- **Permanent actions say "permanent".** A button, menu item or tooltip that deletes without the
  Recycle Bin must say so in its label. "Delete…" next to "Recycle" is not enough.

If your change touches `src/ops.rs`, `src/safety.rs`, `src/app/dialogs.rs`, the delete flow, or the
scanner (`src/scan.rs`, `src/fsread.rs`, `src/tree.rs`), add tests for the new behaviour and write
what could go wrong in **Risks**.

### UI changes

- Add **before and after screenshots of the same folder**, at the same window size, with the same
  item selected. Use a real or generated folder with mixed file types and some nesting. The app also
  builds and runs on Linux (`cargo run -- <folder>`), which is fine for screenshots.
- **Keep every file distinguishable** in the treemap. Neighbouring tiles of the same type must still
  have a visible edge.
- **Keep the current colourful direction** (vivid category colours, shaded tiles, per-extension
  colours for unknown types) unless an issue agreed otherwise first.
- Keep text readable on bright tiles and check that the selection card and list do not shift when
  you select something.

### Code that runs on build machines: call it out

Say so explicitly in **What Changed** and **Risks** if you change any of these, because they run
code on the maintainer's and CI's machines:

- `build.rs`
- `src/icon_design.rs` (compiled into `build.rs` via `include!`, so it runs at build time; it also
  sets the exe icon)
- dependencies in `Cargo.toml` / `Cargo.lock` (say why the crate is needed)
- `.github/workflows/*`, `scripts/*`, `.cargo/config.toml`, `rust-toolchain.toml`

### No committed binaries

Do not commit executables, DLLs, archives, `.ico` files or other build output. The icon is generated
by `build.rs`. Put screenshots in the PR description, not the repo (only update
`docs/screenshot.png` on purpose, when the README image should change).

## PR requirements

### Use the PR template

Every PR must follow [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md). If your
tool skips the template, copy it in by hand. Required sections: **Thinking Path, What Changed,
Verification, Risks, Model Used, Checklist.**

### Model Used (required)

Every PR must say which AI model produced or assisted the change. Include:

- **Provider** (for example Anthropic, OpenAI, Google)
- **Exact model ID or version**, not just a family name
- **Context window**
- **Reasoning or effort mode** (for example high effort, extended thinking, none)
- **Tool or agent harness** (for example Cursor cloud agent, Cursor IDE, Claude Code, Codex CLI,
  Copilot)

If a detail is not visible to you, write "not exposed" instead of guessing. If no AI was involved,
write **"None, human-authored"**. This applies to everyone.

**AI agents must say they are AI** in the PR body (for example "This PR was written by an AI coding
agent"). Ideally, also add a trailer to each commit:

```
Assisted-by: Anthropic claude-opus-4-6, high effort (Claude Code)
```

### Link issues, or describe the problem in the PR

- **Issue exists:** link it with `Fixes #12`, `Closes #12` or `Refs #12`. Link duplicates and
  related PRs too.
- **No issue:** describe it in the PR. For a bug: what happened, what you expected, steps to
  reproduce, disktree version or commit, Windows version. For a feature: the problem, your
  solution, alternatives you considered.

Only reference **public** GitHub issues and PRs. No internal ticket ids (`ABC-123`), links to your
own tracker, agent dashboards, `localhost` or private URLs. Restate useful context in plain words.

### Branch names

Use short, descriptive kebab-case names such as `fix/long-path-recycle`, `docs/contributing` or
`ui/selection-card`. No ticket ids or tool-generated slugs. To rename before pushing:

```bash
git branch -m fix/long-path-recycle
git push -u origin fix/long-path-recycle
```

## Helping other contributors

Picking up a stalled or almost-there PR is **strongly encouraged**. When you do:

- Credit the original author in the PR description and thank them.
- Keep their commits where you reasonably can (or add a `Co-authored-by:` trailer).
- Be kind in reviews. People put real effort in, even when a PR does not land.

## General rules

- One PR = one logical change.
- Clear commits: an imperative subject ("Keep file edges visible in the treemap"), and a body that
  says why.
- Keep the PR title and description meaningful.
- Be kind.

## Writing the PR description

Write PR text in **Simplified Technical English** (ASD-STE100): short sentences, one idea per
sentence, simple words, active voice.

Start with a **Thinking Path**: a few steps from what the project is down to your change. Examples:

> - space-view builds disktree, a Windows treemap for finding and removing what fills a disk.
> - The user selects an item and then deletes it from the selection card.
> - Paths longer than 260 characters cannot go to the Recycle Bin.
> - Today disktree shows a generic error for these paths.
> - This PR detects long paths before the delete and offers permanent delete, with both confirmations.
> - The benefit is a clear message instead of a failed delete.

> - space-view builds disktree, a Windows treemap for finding and removing what fills a disk.
> - The treemap colours files by type, so the user can see what fills a folder.
> - Many game formats (`.ucas`, `.forge`) were grey "Other" tiles.
> - This PR adds these extensions to the Data category and adds tests.
> - The benefit is that game folders show their real contents.

Then fill in the rest of the template: what you changed, how you verified it (with output), and the
risks.

Questions? Open an issue. We are happy to help.
