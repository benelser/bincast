#!/bin/bash
# Run all expect-based init tests in parallel.
# Usage: ./tests/expect/run_all.sh [path-to-bincast-binary]

set -euo pipefail

BINCAST="${1:-target/debug/bincast}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0
FAIL=0
TOTAL=0
PIDS=()
NAMES=()
LOGS=()

if [ ! -f "$BINCAST" ]; then
    echo "Building bincast..."
    cargo build --quiet
fi

BINCAST="$(cd "$(dirname "$BINCAST")" && pwd)/$(basename "$BINCAST")"

# Create a single-crate fixture
setup_single_crate() {
    local dir="$1"
    mkdir -p "$dir/src"
    cat > "$dir/Cargo.toml" << 'CARGOEOF'
[package]
name = "test-tool"
version = "0.1.0"
edition = "2024"
description = "A test tool"
license = "MIT"
repository = "https://github.com/user/test-tool"
CARGOEOF
    echo 'fn main() { println!("hello"); }' > "$dir/src/main.rs"
    (cd "$dir" && git init -q && git add . && git commit -q -m "init" 2>/dev/null) || true
}

# Create a workspace fixture
setup_workspace() {
    local dir="$1"
    cat > "$dir/Cargo.toml" << 'WSEOF'
[workspace]
members = ["crates/*"]

[workspace.package]
version = "0.2.0"
license = "MIT"
repository = "https://github.com/user/test-project"
WSEOF
    mkdir -p "$dir/crates/my-cli/src"
    cat > "$dir/crates/my-cli/Cargo.toml" << 'CLIEOF'
[package]
name = "my-cli"
version.workspace = true
edition = "2024"
CLIEOF
    echo 'fn main() {}' > "$dir/crates/my-cli/src/main.rs"
    mkdir -p "$dir/crates/core/src"
    cat > "$dir/crates/core/Cargo.toml" << 'COREEOF'
[package]
name = "my-core"
version.workspace = true
edition = "2024"
COREEOF
    echo '' > "$dir/crates/core/src/lib.rs"
    (cd "$dir" && git init -q && git add . && git commit -q -m "init" 2>/dev/null) || true
}

launch_test() {
    local test_name="$1"
    local test_script="$2"
    local fixture_type="${3:-single}"

    local tmpdir
    tmpdir=$(mktemp -d "/tmp/bincast-expect-XXXXXX")

    if [ "$fixture_type" = "workspace" ]; then
        setup_workspace "$tmpdir"
    else
        setup_single_crate "$tmpdir"
    fi

    local logfile="$tmpdir/test.log"

    # Run in background
    (cd "$tmpdir" && expect "$test_script" "$BINCAST" > "$logfile" 2>&1; echo $? > "$tmpdir/exit_code") &

    PIDS+=($!)
    NAMES+=("$test_name")
    LOGS+=("$logfile")
    TOTAL=$((TOTAL + 1))
}

echo "Running expect tests in parallel..."
echo ""

launch_test "minimal_profile" "$SCRIPT_DIR/test_init_minimal.exp"
launch_test "rust_ecosystem" "$SCRIPT_DIR/test_init_rust_ecosystem.exp"
launch_test "custom_profile" "$SCRIPT_DIR/test_init_custom.exp"
launch_test "maximum_reach" "$SCRIPT_DIR/test_init_maximum_reach.exp"
launch_test "invalid_input" "$SCRIPT_DIR/test_init_invalid_input.exp"
launch_test "workspace_init" "$SCRIPT_DIR/test_init_workspace.exp" workspace
launch_test "init_then_generate" "$SCRIPT_DIR/test_init_then_generate.exp"

# Wait for all
for i in "${!PIDS[@]}"; do
    wait "${PIDS[$i]}" 2>/dev/null
    local_exit=$(cat "$(dirname "${LOGS[$i]}")/exit_code" 2>/dev/null || echo "1")
    if [ "$local_exit" = "0" ]; then
        echo "  ✓ ${NAMES[$i]}"
        PASS=$((PASS + 1))
    else
        echo "  ✗ ${NAMES[$i]}"
        # Show last line of log (the FAIL message)
        tail -1 "${LOGS[$i]}" 2>/dev/null | sed 's/^/    /'
        FAIL=$((FAIL + 1))
    fi
done

# Clean up
rm -rf /tmp/bincast-expect-*

echo ""
echo "Results: $PASS/$TOTAL passed, $FAIL failed"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
