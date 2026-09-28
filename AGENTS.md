# AGENTS.md

Instructions for coding agents working in this repository (Claude Code, Codex,
Cursor, Copilot, and anything else that reads `AGENTS.md`). Useful for people
too. Follow these over your defaults.

## What this is

**g3-native-plugins** wraps native platform capabilities for Dioxus apps:
clipboard and share, Apple and Google sign-in, external URLs, key-value
storage, camera and microphone permission, geolocation, in-app purchases,
system Back, background media, and deep links (receiving them, and the
metadata files a server hosts for them). Every plugin is behind a feature
flag. Part of the g3 stack; g3-route-transitions' `native-back` builds on the
`back-button` plugin, and the g3 apps use the rest.

| Piece | Version | Reference |
| --- | --- | --- |
| Dioxus | 0.7.9 | [dioxuslabs.com/learn/0.7](https://dioxuslabs.com/learn/0.7/), and g3-stack's `docs/dioxus/patterns.md` |
| Android | Kotlin, one Gradle module per plugin | `src/android/<plugin>/` |
| iOS / macOS | Swift package | `src/ios/` |
| Rust | edition 2024 | `rust-toolchain.toml` |

**Your training data does not know this crate, and is probably wrong about
Dioxus 0.7's native plugin FFI.** The README's support matrix is the contract
with apps and must stay true.

## Map

```
src/lib.rs              NativePlugins (the handles), NativePluginsProvider, and tests
src/<plugin>.rs         The Rust side of each plugin, behind its feature
src/android_bridge.rs   Shared JNI plumbing
src/android/<plugin>/   Kotlin for each plugin (a Gradle module)
src/ios/Sources/        Swift for each plugin
testbed/                A small app that calls every plugin on a device
README.md               Support matrix, and usage for each plugin
CHANGELOG.md            Every user-visible change, under [Unreleased] until a release
```

## Commands

```bash
just check        # default features and all features
just test         # nextest (default and all features) + doc tests
just lint-strict  # clippy with warnings as errors, as CI runs it
just pre-push     # format, check, lint, test, typos
```

## Definition of done

1. `just pre-push` passes.
2. A user-visible change has a line under `## [Unreleased]` in `CHANGELOG.md`,
   and the README's support matrix still matches what each platform does.
3. A change to a platform implementation was run on that platform (the test
   bed on a device or emulator, or a consuming app patched to this checkout),
   or the change says plainly that it was not.
4. Contracts between Rust and Kotlin or Swift (event names, method names,
   message shapes) have a source test in `src/lib.rs`, as the back-button
   ones do.

If you could not do one of these, say which and why.

---

## Rules

### Platforms

- **A plugin with no implementation for a target compiles to an inert no-op**,
  never a build error, so an app's call sites build everywhere. Say in the
  matrix and the plugin's docs where it is inert.
- **Web behaviour is explicit.** Where the browser has its own API
  (geolocation), the plugin is inert on the web on purpose and the docs point
  to the browser API. Where the web fallback is weaker (storage is
  `localStorage`, not encrypted), the docs say so in bold.
- Deep-link metadata builders are not feature-gated and build everywhere,
  including on the server.

### Apps and hydration

- `NativePluginsProvider` renders only its children on every target and
  installs the context only where one exists, so a server-rendered tree and a
  hydrated one match. Keep it that way: **gate calls, never markup.** Document
  every plugin's usage with the `cfg` on the `use_context` line and on the
  call, not on the markup.
- Interception that changes platform behaviour (system Back) is opt-in, and
  falls through to the platform when nothing claims it.

### Dioxus

- Hooks run unconditionally, before any early return; `use_effect` keeps its
  first closure.
- No `use_reactive!`. Never hold a signal borrow across `.await`.
- A handler a plugin stores for later (a Back listener, a purchase update) is
  called through its handle, which stays current, and reads state when it runs.

### Releases

- Releases publish from `publish-crate.yml` (crates.io trusted publishing).
  Do not run `cargo publish` by hand, and do not release without the
  maintainer's go-ahead.
- Commits follow Conventional Commits; lefthook checks them.

## Where to look

- `README.md`: the support matrix and usage for every plugin
- `testbed/README.md`: running the test bed on a device
- `CHANGELOG.md`: what changed between versions
- g3-stack's `docs/native-plugins.md`, for how an app uses them
