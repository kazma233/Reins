// reins-fake-cli 替身的契约测试。同时保证 `cargo test` 会以普通模式构建该
// bin(存在集成测试目标时 cargo 才在 target/<profile>/ 下产出正常 bin 产物),
// session_delete 单元测试的 fake_cli_source fallback 定位依赖这一点。

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("reins-fake-cli-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// 日志写在自身所在目录,拷到隔离目录执行避免污染构建目录。
fn copy_into(dir: &PathBuf) -> PathBuf {
    let source = PathBuf::from(env!("CARGO_BIN_EXE_reins-fake-cli"));
    let destination = dir.join(source.file_name().unwrap());
    fs::copy(&source, &destination).unwrap();
    destination
}

#[test]
fn fake_cli_logs_codex_and_opencode_patterns_and_rejects_unknown() {
    let dir = temp_dir("log");
    let exe = copy_into(&dir);

    let run = |args: &[&str]| Command::new(&exe).args(args).status().unwrap();

    assert!(run(&["delete", "--force", "ses-1"]).success());
    assert!(run(&["session", "delete", "ses-2"]).success());
    assert!(!run(&["totally", "unknown"]).success());

    let log = fs::read_to_string(dir.join("fake-cli.log")).unwrap();
    assert_eq!(log.lines().collect::<Vec<_>>(), ["ses-1", "ses-2"]);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn fake_cli_grok_pattern_removes_session_dir() {
    let dir = temp_dir("grok");
    // grok 形态按自身位置推导 home:<temp>/.grok/sessions/<bucket>/<id>。
    let bucket = dir.join(".grok/sessions/not-a-cwd");
    fs::create_dir_all(bucket.join("grok-solo")).unwrap();
    fs::write(bucket.join("grok-solo/summary.json"), "{}").unwrap();

    let bin_dir = dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let exe = copy_into(&bin_dir);

    let status = Command::new(&exe)
        .args(["sessions", "delete", "grok-solo"])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!bucket.join("grok-solo").exists());
    assert_eq!(
        fs::read_to_string(bin_dir.join("fake-cli.log"))
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        ["grok-solo"]
    );

    fs::remove_dir_all(&dir).ok();
}
