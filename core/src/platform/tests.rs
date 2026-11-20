/// The program sees the C locale, whatever the caller's environment says.
#[cfg(unix)]
#[test]
fn a_parsed_subprocess_runs_in_the_c_locale() {
    let output = super::parsed_output_command("/usr/bin/env")
        .env("LANG", "de_DE.UTF-8")
        .output()
        .unwrap();

    let environment = String::from_utf8(output.stdout).unwrap();
    assert!(
        environment.lines().any(|line| line == "LC_ALL=C"),
        "{environment}"
    );
}
