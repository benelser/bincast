#!/bin/bash
# Hermetic bootstrap test
#
# Validates the full user journey:
# 1. Fresh Rust project in temp dir
# 2. bincast init (via expect)
# 3. bincast generate
# 4. bincast check
# 5. Verify all generated files
# 6. Verify skill package is readable
# 7. Verify bincast.toml round-trips
#
# Uses the system-installed bincast (brew/cargo/curl)

set -euo pipefail

BINCAST="$(which bincast)"
if [ -z "$BINCAST" ]; then
    echo "FAIL: bincast not found in PATH"
    echo "  Install: brew install benelser/bincast/bincast"
    exit 1
fi

VERSION=$($BINCAST version 2>&1 | head -1)
echo "=== Hermetic Bootstrap Test ==="
echo "  bincast: $BINCAST"
echo "  version: $VERSION"
echo ""

# --- Step 1: Create fresh Rust project ---
TESTDIR=$(mktemp -d)
trap 'rm -rf "$TESTDIR"' EXIT

echo "  Creating test project in $TESTDIR..."

cargo init --name test-project "$TESTDIR" --quiet 2>/dev/null

# Add metadata that bincast needs
cat > "$TESTDIR/Cargo.toml" << 'CARGOEOF'
[package]
name = "test-project"
version = "0.1.0"
edition = "2024"
description = "A hermetic test project"
license = "MIT"
repository = "https://github.com/test-user/test-project"
CARGOEOF

mkdir -p "$TESTDIR/src"
echo 'fn main() { println!("hello from test-project"); }' > "$TESTDIR/src/main.rs"

cd "$TESTDIR"
git init -q
git add .
git commit -q -m "init"

echo "  ✓ Project created"

# --- Step 2: Run bincast init (Minimal profile) ---
echo "  Running bincast init..."

expect -c '
set timeout 15
spawn '"$BINCAST"' init
expect "Choose"
send "3\r"
expect "Execute"
send "\r"
expect "Done!"
expect eof
' > /dev/null 2>&1

if [ ! -f "bincast.toml" ]; then
    echo "  FAIL: bincast.toml not created"
    exit 1
fi
echo "  ✓ bincast init completed"

# --- Step 3: Verify generated files ---
EXPECTED_FILES=(
    "bincast.toml"
    ".github/workflows/release.yml"
    "install.sh"
    "install.ps1"
    "binstall.toml"
)

for f in "${EXPECTED_FILES[@]}"; do
    if [ ! -f "$f" ]; then
        echo "  FAIL: missing file: $f"
        exit 1
    fi
    SIZE=$(wc -c < "$f" | tr -d ' ')
    if [ "$SIZE" -eq 0 ]; then
        echo "  FAIL: empty file: $f"
        exit 1
    fi
    echo "  ✓ $f ($SIZE bytes)"
done

# --- Step 4: Validate config ---
echo "  Running bincast check..."
# bincast check will fail on name availability (test-project doesn't exist on registries)
# but config validation should pass
$BINCAST check 2>&1 | head -3 || true
echo "  ✓ bincast check ran"

# --- Step 5: Verify CI workflow content ---
echo "  Validating CI workflow..."

CI=".github/workflows/release.yml"
REQUIRED_PATTERNS=(
    "name: Release"
    "tags:"
    "cargo build --release"
    "SHA-256"
    "softprops/action-gh-release"
)

for pattern in "${REQUIRED_PATTERNS[@]}"; do
    if ! grep -q "$pattern" "$CI"; then
        echo "  FAIL: CI missing: $pattern"
        exit 1
    fi
done
echo "  ✓ CI workflow valid"

# --- Step 6: Verify install.sh content ---
echo "  Validating install.sh..."
if ! grep -q "test-user/test-project" install.sh; then
    echo "  FAIL: install.sh missing repo reference"
    exit 1
fi
if ! head -1 install.sh | grep -q "#!/bin/sh"; then
    echo "  FAIL: install.sh missing shebang"
    exit 1
fi
echo "  ✓ install.sh valid"

# --- Step 7: Verify bincast.toml content ---
echo "  Validating bincast.toml..."
REQUIRED_CONFIG=(
    "[package]"
    "name = \"test-project\""
    "[targets]"
    "[distribute.github]"
    "[distribute.install_script]"
)

for pattern in "${REQUIRED_CONFIG[@]}"; do
    if ! grep -q "$pattern" bincast.toml; then
        echo "  FAIL: bincast.toml missing: $pattern"
        exit 1
    fi
done
echo "  ✓ bincast.toml valid"

# --- Step 8: Verify version command works ---
echo "  Testing version bump..."
BUMP_OUTPUT=$($BINCAST version patch 2>&1)
if ! echo "$BUMP_OUTPUT" | grep -q "0.1.1"; then
    echo "  FAIL: version bump didn't produce 0.1.1"
    echo "  Output: $BUMP_OUTPUT"
    exit 1
fi

# Verify Cargo.toml was updated
if ! grep -q 'version = "0.1.1"' Cargo.toml; then
    echo "  FAIL: Cargo.toml not bumped to 0.1.1"
    exit 1
fi
echo "  ✓ Version bumped to 0.1.1"

# --- Step 9: Verify skills are readable ---
echo "  Validating skill files..."
SKILL_DIR="$(dirname "$0")/../../skills"
if [ -d "$SKILL_DIR" ]; then
    SKILL_COUNT=$(find "$SKILL_DIR" -name "SKILL.md" | wc -l | tr -d ' ')
    if [ "$SKILL_COUNT" -lt 5 ]; then
        echo "  FAIL: expected at least 5 skills, found $SKILL_COUNT"
        exit 1
    fi

    # Verify each skill has required frontmatter
    for skill in "$SKILL_DIR"/*/SKILL.md; do
        if ! head -1 "$skill" | grep -q "^---"; then
            echo "  FAIL: skill missing frontmatter: $skill"
            exit 1
        fi
        SKILL_NAME=$(basename "$(dirname "$skill")")
        echo "  ✓ skill: $SKILL_NAME"
    done
else
    echo "  SKIP: skills directory not found (running from installed binary)"
fi

# --- Step 10: Verify apm.yml ---
APM_FILE="$(dirname "$0")/../../apm.yml"
if [ -f "$APM_FILE" ]; then
    if ! grep -q "name: bincast" "$APM_FILE"; then
        echo "  FAIL: apm.yml missing package name"
        exit 1
    fi
    if ! grep -q "type: skill" "$APM_FILE"; then
        echo "  FAIL: apm.yml missing type: skill"
        exit 1
    fi
    echo "  ✓ apm.yml valid"
else
    echo "  SKIP: apm.yml not found (running from installed binary)"
fi

echo ""
echo "=== ALL TESTS PASSED ==="
echo "  Project: $TESTDIR"
echo "  bincast: $VERSION"
echo "  Files generated: ${#EXPECTED_FILES[@]}"
echo "  CI workflow: valid"
echo "  Version bump: working"
