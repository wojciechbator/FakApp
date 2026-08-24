# FakApp — command runner. `just` alone lists recipes.
default:
    @just --list

# Format check
fmt:
    cargo fmt --all --check

fmt-fix:
    cargo fmt --all

# Clippy, warnings deny
clippy:
    cargo clippy --all-targets -- -D warnings

# Unit tests
test:
    cargo test

# A watchdog that panics dies quietly; this gate keeps runtime code clean.
panics:
    python3 scripts/check_runtime_panics.py

# Everything that must pass before a commit ships.
check: fmt clippy panics test

# Release build (locked)
build:
    cargo build --release --locked

# Ship to virya-oracle: static musl binary via docker, install, verify.
deploy:
    scripts/deploy.sh

# Previous binary back, restart, verify.
rollback:
    scripts/deploy.sh rollback
