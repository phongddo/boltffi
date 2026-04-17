#!/bin/bash
# Builds the rust-boltffi cdylib, regenerates the C# bindings, and runs the
# BenchmarkDotNet suite. Arguments after `--` are forwarded to BenchmarkSwitcher
# (e.g. `./build.sh -- --filter '*String*'`).
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR/../.."
RUST_DIR="$SCRIPT_DIR/../rust-boltffi"

SKIP_BENCH=false
BENCH_ARGS=()

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-bench)
            SKIP_BENCH=true
            shift
            ;;
        --)
            shift
            BENCH_ARGS=("$@")
            break
            ;;
        -h|--help)
            echo "Usage: $0 [--skip-bench] [-- <BenchmarkDotNet args>]"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

echo "=== Building boltffi CLI (debug) ==="
cargo build -p boltffi_cli --manifest-path "$ROOT_DIR/Cargo.toml"

echo "=== Building rust-boltffi cdylib (release) ==="
cargo build --release --manifest-path "$RUST_DIR/Cargo.toml"

echo "=== Generating C# bindings ==="
cd "$RUST_DIR"
"$ROOT_DIR/target/debug/boltffi" generate csharp --experimental

if [[ "$SKIP_BENCH" == true ]]; then
    echo "=== Skipping benchmark run (--skip-bench) ==="
    exit 0
fi

echo "=== Running BenchmarkDotNet ==="
cd "$SCRIPT_DIR"
dotnet run -c Release -- "${BENCH_ARGS[@]}"
