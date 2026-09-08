set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
    @just --list

setup:
    lefthook install

format:
    cargo fmt --all

format-check:
    cargo fmt --all -- --check

check:
    cargo check
    cargo check --all-features

lint:
    cargo clippy --all-targets --no-deps
    cargo clippy --all-targets --all-features --no-deps

lint-strict:
    cargo clippy --all-targets --no-deps -- -D warnings
    cargo clippy --all-targets --all-features --no-deps -- -D warnings

test:
    cargo nextest run
    cargo nextest run --all-features
    cargo test --doc --all-features

spell:
    typos

security:
    cargo deny check

pre-push: format-check check lint-strict test spell

quality: pre-push

package-check:
    cargo package --allow-dirty

ci: quality security package-check

package:
    cargo package
