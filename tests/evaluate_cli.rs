use std::process::Command;

#[test]
fn registry_commands_keep_their_existing_input_flags() {
    let binary = env!("CARGO_BIN_EXE_evaluate");
    let help = Command::new(binary).output().unwrap();
    let help = String::from_utf8(help.stderr).unwrap();
    for (command, input) in [
        ("sample-us", "nad"),
        ("sample-fr", "ban"),
        ("sample-no", "kartverket"),
        ("sample-ch", "swisstopo"),
        ("sample-nl", "bag"),
        ("sample-kr", "schools"),
        ("sample-cz", "ruian"),
    ] {
        assert!(help.contains(command) && help.contains(&format!("--{input} FILE")));
        let output = Command::new(binary)
            .args([command, &format!("--{input}"), "unused"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("--seed is required"), "{command}: {error}");
    }
}
