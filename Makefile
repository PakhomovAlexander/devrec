.PHONY: check check-local check-native build test install
# AF supplies an absolute writable scratch directory for sealed, read-only sources.
# Never create target/ in the sealed tree. The private copy owns all build outputs.
check:
	@if [ -n "$${AF_CHECK_SCRATCH:-}" ]; then sh scripts/check-sealed.sh; else $(MAKE) check-local; fi
check-native:
	DEVREC_REQUIRE_NATIVE_AF=1 $(MAKE) check
check-local:
	mkdir -p target/tmp
	CARGO_HOME="$(CURDIR)/target/cargo-home" TMPDIR="$(CURDIR)/target/tmp" cargo fmt --check
	CARGO_HOME="$(CURDIR)/target/cargo-home" TMPDIR="$(CURDIR)/target/tmp" cargo clippy --locked --all-targets -- -D warnings
	CARGO_HOME="$(CURDIR)/target/cargo-home" TMPDIR="$(CURDIR)/target/tmp" cargo test --locked
build:
	mkdir -p target/tmp
	CARGO_HOME="$(CURDIR)/target/cargo-home" TMPDIR="$(CURDIR)/target/tmp" cargo build --release --locked
test:
	$(MAKE) check
install:
	mkdir -p target/tmp
	CARGO_HOME="$(CURDIR)/target/cargo-home" TMPDIR="$(CURDIR)/target/tmp" cargo install --path . --root target/install --locked
