#!/bin/sh
set -eu
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
make check-local
make install
"$work/source/target/install/bin/devrec" --help
# Re-run integration coverage through the privately installed CLI.
DEVREC_TEST_EXECUTABLE="$work/source/target/install/bin/devrec" \
CARGO_HOME="$work/source/target/cargo-home" TMPDIR="$work/source/target/tmp" \
cargo test --locked --offline --test cli --test native --test native_package
