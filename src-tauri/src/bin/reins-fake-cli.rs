//! 测试专用的假 CLI:session 删除测试用它顶替 codex/grok/opencode 的官方
//! 删除命令,让同一套测试在所有平台运行(Windows 无法执行 sh 脚本,而
//! CreateProcess 只按 .exe 解析 PATH 查找,.cmd 脚本也顶替不了)。
//!
//! 行为契约(与被模拟的官方命令对齐):
//! - codex `delete --force <id>`、opencode `session delete <id>`:记录 id 退出 0;
//! - grok `sessions delete <id>`:记录 id,并删除 sessions 分桶下同名会话目录
//!   (官方命令会清理 root 目录,不模拟会让删除后的目录断言失真);
//! - 其余参数一律 exit 1,模拟命令失败。
//!
//! 日志写在自身所在目录(拷贝件按测试隔离在各 temp 目录),不用环境变量传
//! 路径,避免并行测试互串。

use std::io::Write;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // 模式匹配的字面量是 &str,args 元素是 String,用守卫而不是字面量模式。
    let Some(session_id) = recognized_delete_session_id(&args) else {
        std::process::exit(1);
    };

    log_session_id(&session_id);
    if args.first().map(String::as_str) == Some("sessions") {
        delete_grok_session_dirs(&session_id);
    }
    std::process::exit(0);
}

// codex `delete --force <id>` / grok `sessions delete <id>` /
// opencode `session delete <id>` 的统一提取。
fn recognized_delete_session_id(args: &[String]) -> Option<String> {
    let [first, second, session_id] = args else {
        return None;
    };
    match (first.as_str(), second.as_str()) {
        ("delete", "--force") | ("sessions", "delete") | ("session", "delete") => {
            Some(session_id.clone())
        }
        _ => None,
    }
}

fn log_session_id(session_id: &str) {
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(own_dir().join("fake-cli.log"))
    else {
        return;
    };
    let _ = writeln!(file, "{session_id}");
}

// 假 CLI 被测试拷贝到 <temp>/bin 下,grok home 是 temp 的 .grok 目录。
fn delete_grok_session_dirs(session_id: &str) {
    let Some(sessions_root) = own_dir().parent().map(|dir| dir.join(".grok/sessions")) else {
        return;
    };
    let Ok(buckets) = std::fs::read_dir(&sessions_root) else {
        return;
    };
    for bucket in buckets.flatten() {
        let session_dir = bucket.path().join(session_id);
        if session_dir.is_dir() {
            let _ = std::fs::remove_dir_all(&session_dir);
        }
    }
}

fn own_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}
