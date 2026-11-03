//! End-to-end: the real `bymm` executable is embedded with data, then *run*.

mod common;

use common::TempDir;
use std::path::Path;
use std::process::{Command, Output};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_bymm")
}

fn run(exe: impl AsRef<Path>, args: &[&str]) -> Output {
    Command::new(exe.as_ref()).args(args).output().unwrap()
}

fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn copy_of_cli(dir: &TempDir) -> std::path::PathBuf {
    let p = dir
        .path()
        .join(if cfg!(windows) { "app.exe" } else { "app" });
    std::fs::copy(bin(), &p).unwrap();
    p
}

#[test]
fn a_binary_with_embedded_data_still_executes_and_reads_its_own_data() {
    let dir = TempDir::new();
    let app = copy_of_cli(&dir);
    bymm::write(&app, "greeting", "hello from inside the binary").unwrap();

    // The embedded program runs and reads its own bytes.
    let out = run(&app, &["self-get", "greeting"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(text(&out), "hello from inside the binary");
}

#[test]
fn a_running_executable_can_rewrite_itself() {
    let dir = TempDir::new();
    let app = copy_of_cli(&dir);

    assert!(run(&app, &["self-set", "counter", "1"]).status.success());
    assert_eq!(text(&run(&app, &["self-get", "counter"])), "1");
    // second round: the file being replaced was itself produced by a commit
    assert!(run(&app, &["self-set", "counter", "2"]).status.success());
    assert_eq!(text(&run(&app, &["self-get", "counter"])), "2");

    // visible from the library, and still a valid executable
    assert_eq!(bymm::read(&app, "counter").unwrap().unwrap(), b"2");
    assert!(run(&app, &["help"]).status.success());
}

#[test]
fn cli_commands() {
    let dir = TempDir::new();
    let app = copy_of_cli(&dir);
    let app_s = app.to_str().unwrap();
    let original = std::fs::read(&app).unwrap();

    let info = run(bin(), &["info", app_s]);
    assert!(info.status.success());
    let t = text(&info);
    assert!(t.contains("format"), "{t}");
    assert!(t.contains("no storage"), "{t}");

    assert!(
        run(bin(), &["set", app_s, "name", "yolezz"])
            .status
            .success()
    );
    assert!(run(bin(), &["append", app_s, "name", "!"]).status.success());
    assert_eq!(text(&run(bin(), &["get", app_s, "name"])), "yolezz!");
    assert!(text(&run(bin(), &["ls", app_s])).contains("name"));
    assert!(run(bin(), &["verify", app_s]).status.success());

    assert!(run(bin(), &["limit", app_s, "4096"]).status.success());
    assert_eq!(text(&run(bin(), &["limit", app_s])).trim(), "4096");
    let too_big = "x".repeat(5000);
    let o = run(bin(), &["set", app_s, "big", &too_big]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("limit"));
    assert!(run(bin(), &["limit", app_s, "unlimited"]).status.success());
    assert_eq!(text(&run(bin(), &["limit", app_s])).trim(), "unlimited");

    assert_eq!(
        run(bin(), &["get", app_s, "missing"]).status.code(),
        Some(1)
    );
    assert!(run(bin(), &["rm", app_s, "name"]).status.success());
    assert!(run(bin(), &["clear", app_s]).status.success());
    assert!(run(bin(), &["strip", app_s]).status.success());
    assert_eq!(std::fs::read(&app).unwrap(), original);

    assert_eq!(run(bin(), &["bogus"]).status.code(), Some(2));
    assert_eq!(run(bin(), &[]).status.code(), Some(2));
}

#[test]
#[cfg(unix)]
fn a_system_binary_can_be_embedded_in_a_copy() {
    // /bin/true is a real, dynamically linked system executable.
    let Some(src) = ["/bin/true", "/usr/bin/true"]
        .into_iter()
        .find(|p| Path::new(p).exists())
    else {
        return;
    };
    let dir = TempDir::new();
    let copy = dir.path().join("true-copy");
    std::fs::copy(src, &copy).unwrap();
    let info = bymm::see::info(&copy).unwrap();
    bymm::write(&copy, "k", vec![42u8; 123_456]).unwrap();
    assert_eq!(bymm::see::info(&copy).unwrap().image_end, info.image_end);
    assert!(Command::new(&copy).status().unwrap().success());
    assert_eq!(
        bymm::read(&copy, "k").unwrap().unwrap(),
        vec![42u8; 123_456]
    );
}
