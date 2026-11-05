//! Exercise the sealed harness path plumbing without recursively building Rust.
#![cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

#[test]
fn aliased_scratch_uses_physical_installed_paths_and_cleans_up() {
    let owner = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(owner.path()).unwrap();
    let scratch = base.join("scratch");
    let alias = base.join("alias");
    let bin = base.join("bin");
    fs::create_dir(&scratch).unwrap();
    fs::create_dir(&bin).unwrap();
    std::os::unix::fs::symlink(&scratch, &alias).unwrap();
    let make = r#"#!/bin/sh
set -eu
test -z "${AF_CHECK_SCRATCH:-}"
test "$PWD" = "$(pwd -P)"
case "$1" in
check-local) mkdir -p target/tmp ;;
install)
    mkdir -p target/install/bin
    printf '#!/bin/sh\nexit 0\n' > target/install/bin/devrec
    chmod +x target/install/bin/devrec
    ;;
*) exit 91 ;;
esac
"#;
    let cargo = r#"#!/bin/sh
set -eu
physical=$(pwd -P)
test "$TMPDIR" = "$physical/target/tmp"
test "$CARGO_HOME" = "$physical/target/cargo-home"
test "$DEVREC_TEST_EXECUTABLE" = "$physical/target/install/bin/devrec"
test "$(cd "$TMPDIR" && pwd -P)" = "$TMPDIR"
test "$*" = 'test --locked --offline --test cli --test native --test native_package'
printf 'installed paths checked\n' >> "$HARNESS_LOG"
exit "$HARNESS_EXIT"
"#;
    for (name, script) in [("make", make), ("cargo", cargo)] {
        let path = bin.join(name);
        fs::write(&path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let path = std::env::join_paths(
        std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let log = base.join("log");
    // Both a passing installed check and a failed one must remove private scratch.
    for code in [0, 42] {
        let output = Command::new("sh")
            .arg("scripts/check-sealed.sh")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("PATH", &path)
            .env("AF_CHECK_SCRATCH", &alias)
            .env("HARNESS_LOG", &log)
            .env("HARNESS_EXIT", code.to_string())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0);
    }
    assert_eq!(
        fs::read_to_string(log).unwrap(),
        "installed paths checked\ninstalled paths checked\n"
    );
}
