# Changelog

All notable changes to `dx-native-plugins` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-20

Initial release.

- Feature-gated native plugins: `clipboard` (copy and share sheet), `auth`
  (Sign in with Apple, Google Sign-In), `external-url`, `back-button`, and
  `media` (background playback).
- `deep_links` builders for `apple-app-site-association` and
  `assetlinks.json`, plus route macros that serve them.
- Platform coverage is uneven; see the support matrix in the README. Notably,
  `media` and `back-button` are Android-only.
