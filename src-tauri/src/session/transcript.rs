//! 转录文件的物理形态适配。
//!
//! 两家来源会把磁盘上的转录压缩,读取方必须按各自契约处理:
//! - codex 的后台 worker 把 7 天未修改的冷会话原地压成 `.jsonl.zst` 并删除
//!   明文(上游 `rollout/src/compression.rs`),归档会话的文件在
//!   `sessions/` 与 `archived_sessions/` 之间移动;
//! - dsh 的转录按写入批次写成多帧 `.jsonl.zstd`。
//!
//! 路径归一与解码收在这里,按行循环不再各写一遍 zstd。

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// codex 压缩转录的后缀:上游给整个 rollout 文件名追加 `.zst`。
pub(crate) const CODEX_COMPRESSED_SUFFIX: &str = ".jsonl.zst";
/// dsh 压缩转录的后缀。
pub(crate) const DSH_COMPRESSED_SUFFIX: &str = ".jsonl.zstd";

/// 该路径是否为压缩形态的转录。
pub(crate) fn is_compressed(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    name.ends_with(CODEX_COMPRESSED_SUFFIX) || name.ends_with(DSH_COMPRESSED_SUFFIX)
}

/// 转录当前的物理位置:明文优先,其次压缩兄弟文件——与 codex 上游
/// `existing_rollout_path` 同语义。压缩/解压会改写这两种拼写,读取方拿到的
/// 路径可能是上一轮的形态,按此归一后再打开,不必各自重扫目录。
pub(crate) fn resolve(path: &Path) -> PathBuf {
    if path.is_file() {
        return path.to_path_buf();
    }

    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    // 记的是压缩路径而磁盘上已还原成明文(codex 续跑会先 materialize)。
    for suffix in [CODEX_COMPRESSED_SUFFIX, DSH_COMPRESSED_SUFFIX] {
        if let Some(plain) = name.strip_suffix(suffix) {
            let candidate = path.with_file_name(plain);
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    // 记的是明文路径而磁盘上已压缩(codex 的冷会话压缩)。
    for suffix in [CODEX_COMPRESSED_SUFFIX, DSH_COMPRESSED_SUFFIX] {
        if !name.ends_with(suffix) {
            let candidate = path.with_file_name(format!("{name}{suffix}"));
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    path.to_path_buf()
}

/// 打开按行读取的转录:压缩形态边解边读,只读前缀的调用方(如 codex 索引
/// 扫描只取首行 session_meta)不必为一行解压整个文件。
pub(crate) fn open_lines(path: &Path) -> Result<Box<dyn std::io::BufRead>> {
    let resolved = resolve(path);
    let file =
        File::open(&resolved).with_context(|| format!("Failed to open {}", resolved.display()))?;

    if is_compressed(&resolved) {
        let decoder = zstd::stream::read::Decoder::new(BufReader::new(file))
            .with_context(|| format!("Failed to decode {}", resolved.display()))?;
        Ok(Box::new(BufReader::new(decoder)))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

/// 读取转录全文:压缩形态整份解码后返回原始字节。
pub(crate) fn read_frames(path: &Path) -> Result<Vec<u8>> {
    let resolved = resolve(path);
    let file =
        File::open(&resolved).with_context(|| format!("Failed to open {}", resolved.display()))?;

    if !is_compressed(&resolved) {
        let mut raw = Vec::new();
        BufReader::new(file)
            .read_to_end(&mut raw)
            .with_context(|| format!("Failed to read {}", resolved.display()))?;
        return Ok(raw);
    }

    decode_all(file, &resolved)
}

// 解码的容错口径:live 转录的尾帧可能截断(正在写入),截断前已解出的内容
// 照常返回,只有第一段就失败才判定为文件损坏。dsh 的多帧拼接与 codex 的
// 单帧都走这条路径。
fn decode_all(file: File, path: &Path) -> Result<Vec<u8>> {
    let mut decoder = zstd::stream::read::Decoder::new(BufReader::new(file))
        .with_context(|| format!("Failed to decode {}", path.display()))?;
    let mut output = Vec::new();
    let mut buffer = vec![0u8; 64 * 1024];

    loop {
        match decoder.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => output.extend_from_slice(&buffer[..read]),
            Err(error) => {
                if output.is_empty() {
                    bail!(
                        "Failed to decode zstd transcript {}: {error}",
                        path.display()
                    );
                }
                break;
            }
        }
    }

    Ok(output)
}
