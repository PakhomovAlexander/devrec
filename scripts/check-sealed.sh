#!/bin/sh
set -eu
case "${AF_CHECK_SCRATCH:?AF_CHECK_SCRATCH required}" in /*) ;; *) echo 'AF_CHECK_SCRATCH must be absolute' >&2; exit 1;; esac
work=$(mktemp -d "$AF_CHECK_SCRATCH/devrec-check.XXXXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
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
