#!/bin/sh
set -eu
# Sealed AF acceptance requires the real native kernel, including installed-CLI
# replay. Do not inherit an optional standalone coverage policy from the caller.
export DEVREC_REQUIRE_NATIVE_AF=1
case "${AF_CHECK_SCRATCH:?AF_CHECK_SCRATCH required}" in /*) ;; *) echo 'AF_CHECK_SCRATCH must be absolute' >&2; exit 1;; esac
work=$(mktemp -d "$AF_CHECK_SCRATCH/devrec-check.XXXXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
# AF scratch may use a symlinked ancestor (notably /var on macOS). Keep all
# explicit installed-test paths physical, matching make's canonical CURDIR.
work=$(CDPATH= cd "$work" && pwd -P)
mkdir "$work/source"
# Explicit authoritative files; no target, Git metadata or machine-local state.
for entry in Cargo.toml Cargo.lock Makefile src tests scripts schemas docs integration references README.md; do
    cp -R "$entry" "$work/source/$entry"
done
chmod -R u+w "$work/source"
unset AF_CHECK_SCRATCH
cd "$work/source"
echo 'sealed AF gate: native AF execution required (DEVREC_REQUIRE_NATIVE_AF=1)'
make check-local
make install
"$work/source/target/install/bin/devrec" --help
# Re-run integration coverage through the privately installed CLI.
echo 'sealed AF gate: installed CLI replay with mandatory native AF execution'
DEVREC_TEST_EXECUTABLE="$work/source/target/install/bin/devrec" \
CARGO_HOME="$work/source/target/cargo-home" TMPDIR="$work/source/target/tmp" \
cargo test --locked --offline --test cli --test native --test native_package
