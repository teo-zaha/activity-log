//! `generate-ts --check` as `pnpm verify` runs it: the binary, its exit code and the files it
//! leaves.

use std::process::Command;

fn generate_ts(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_generate-ts"))
        .args(args)
        .output()
        .expect("generate-ts runs")
}

#[test]
fn the_check_fails_on_a_stale_file_and_leaves_it() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("activity.ts");
    let out_arg = out.to_str().unwrap();
    assert!(generate_ts(&["--out", out_arg]).status.success());
    assert!(generate_ts(&["--out", out_arg, "--check"]).status.success());

    std::fs::write(&out, "export type Stale = 1\n").unwrap();
    let checked = generate_ts(&["--out", out_arg, "--check"]);

    assert_eq!(checked.status.code(), Some(1));
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        "export type Stale = 1\n"
    );
}

#[test]
fn the_check_fails_on_missing_files_and_writes_none() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("activity.ts");

    let checked = generate_ts(&["--out", out.to_str().unwrap(), "--check"]);

    assert_eq!(checked.status.code(), Some(1));
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
}
