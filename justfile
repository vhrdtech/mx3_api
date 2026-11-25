# mx3_api: mx3_api_core and mx3_api_lab, the public wire types and clients of the mx3 API

# `just jobs=N <recipe>` caps build and test parallelism for that run; unset (the default) leaves CARGO_BUILD_JOBS
# and NEXTEST_TEST_THREADS as inherited, or at cargo's default (the CPU count)
jobs := ""
export CARGO_BUILD_JOBS := if jobs != "" { jobs } else { env_var_or_default('CARGO_BUILD_JOBS', num_cpus()) }
export NEXTEST_TEST_THREADS := if jobs != "" { jobs } else { env_var_or_default('NEXTEST_TEST_THREADS', 'num-cpus') }

# Bare `just` only lists the recipes
default:
    @just --list

# The tests. `just test`: all of them (test-all), the run of the landing gate. `just test --changed`: only the crates
# changed against main and those that depend on them, before calling a change ready.
test *flags:
    {{ if flags == "--changed" { "changedtests run" } else { just_executable() + " test-all " + flags } }}

# Every test, with no features and with all of them: nextest for unit tests, cargo test for doctests
test-all:
    cargo nextest run --workspace --no-tests=pass
    cargo nextest run --workspace --all-features --no-tests=pass
    cargo test --workspace --all-features --doc

# Build each feature set: none, client, sqlx, all
check-features:
    cargo check --workspace --all-targets
    cargo check --workspace --all-targets --features mx3_api_lab/client
    cargo check --workspace --all-targets --features mx3_api_lab/sqlx
    cargo check --workspace --all-targets --all-features

# Format check, clippy without warnings in each feature set, spl check
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    spl check --progress never

# Format all code and TOML
fmt:
    cargo fmt --all
    taplo fmt

# Show the dependency tree of mx3_api_lab with no features: it must hold no reqwest, tokio or sqlx
tree-no-features:
    cargo tree -p mx3_api_lab -e normal,build

# Move every git dependency to the commit its reference names (commit the lock)
deps-pull:
    cargo update -p rvariant -p mpn_schemes -p si_dynamic
