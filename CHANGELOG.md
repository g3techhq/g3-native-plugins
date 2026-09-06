# Changelog

All notable changes to `g3-native-plugins` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-09-06

Initial release.

- Standardized Android system Back as the cancelable `g3nativeback` DOM event
  so route libraries and higher-priority UI layers can share one event
  contract without app-specific bridge code.
- Made `BackButton::new` public for integration crates that need to own the
  plugin outside `NativePluginsProvider`.
- Added one-press Android fallthrough so a router integration can preserve the
  operating system's normal Back behavior if transient UI and route history
  both decline an intercepted request.
- Feature-gated native plugins: `clipboard` (copy and share sheet), `auth`
  (Sign in with Apple, Google Sign-In), `external-url`, `back-button`, and
  `media` (background playback).
- `deep_links` builders for `apple-app-site-association` and
  `assetlinks.json`, plus route macros that serve them.
- Platform coverage is uneven; see the support matrix in the README. Notably,
  `media` and `back-button` are Android-only.
