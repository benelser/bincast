#!/bin/bash
# Run all expect-based init tests.
# Usage: ./tests/expect/run_all.sh [path-to-bincast-binary]

set -euo pipefail

BINCAST="${1:-target/debug/bincast}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0
FAIL=0

if [ ! -f "$BINCAST" ]; then
    echo "Building bincast..."
    cargo build --quiet
fi

BINCAST="$(cd "$(dirname "$BINCAST")" && pwd)/$(basename "$BINCAST")"

run_test() {
    local test_name="$1"
    local test_script="$2"

    # Arrange: create temp fixture project
    local tmpdir
    tmpdir=$(mktemp -d "/tmp/bincast-expect-${test_name}-XXXXXX")

    cat > "$tmpdir/Cargo.toml" << 'CARGOEOF'
[package]
name = "test-tool"
version = "0.1.0"
edition = "2024"
description = "A test tool"
license = "MIT"
repository = "https://github.com/user/test-tool"
CARGOEOF
    mkdir -p "$tmpdir/src"
    echo 'fn main() { println!("hello"); }' > "$tmpdir/src/main.rs"

    # Act + Assert: run expect script in the fixture dir
    echo -n "  $test_name... "
    if (cd "$tmpdir" && expect "$test_script" "$BINCAST" 2>&1); then
        PASS=$((PASS + 1))
    else
        echo "  FAILED"
        FAIL=$((FAIL + 1))
    fi

    rm -rf "$tmpdir"
}

echo "Running expect-based init tests..."
echo ""

run_test "minimal_profile" "$SCRIPT_DIR/test_init_minimal.exp"
run_test "rust_ecosystem_profile" "$SCRIPT_DIR/test_init_rust_ecosystem.exp"
run_test "custom_profile" "$SCRIPT_DIR/test_init_custom.exp"

echo ""
echo "Results: $PASS passed, $FAIL failed"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
