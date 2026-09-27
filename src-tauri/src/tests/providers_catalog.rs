// 模型目录解析与 models.dev 匹配的纯逻辑测试（不联网）。

use anyhow::Result;
use serde_json::json;

use super::{
    ModelsDevCatalog, match_modelsdev_in_catalog, parse_models_payload, parse_modelsdev_catalog,
};
use crate::providers::types::{ModelsDevMatchStatus, ReasoningLevel};

#[test]
fn parses_openai_and_array_payloads() -> Result<()> {
    let openai = json!({"data": [
        {"id": "b/model", "display_name": "B Model"},
        {"id": "a/model"},
        {"id": "a/model"},
        {"no-id": true}
    ]});
    let models = parse_models_payload(&openai)?;
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "a/model");
    assert_eq!(models[1].name.as_deref(), Some("B Model"));

    let array = json!([{"id": "x"}]);
    assert_eq!(parse_models_payload(&array)?.len(), 1);
    Ok(())
}

#[test]
fn missing_data_includes_upstream_error_detail() {
    // 智谱形态：HTTP 200 + {"code":...,"msg":...,"success":false}
    let zhipu = json!({"code": 401, "msg": "令牌已过期或验证不正确", "success": false});
    let error = parse_models_payload(&zhipu).unwrap_err().to_string();
    assert!(error.contains("缺少 data 字段"));
    assert!(error.contains("code=401"));
    assert!(error.contains("令牌已过期或验证不正确"));

    // OpenAI 形态：嵌套 error.message
    let openai =
        json!({"error": {"message": "Incorrect API key provided", "code": "invalid_api_key"}});
    let error = parse_models_payload(&openai).unwrap_err().to_string();
    assert!(error.contains("缺少 data 字段"));
    assert!(error.contains("code=invalid_api_key"));
    assert!(error.contains("Incorrect API key provided"));

    // 无可识别错误字段时给出兜底提示，不 panic
    let unknown = json!({"success": false});
    let error = parse_models_payload(&unknown).unwrap_err().to_string();
    assert!(error.contains("缺少 data 字段"));
    assert!(error.contains("无法从返回中识别错误原因"));
}

fn fixture_catalog() -> ModelsDevCatalog {
    let data = json!({
        "openrouter": {
            "models": {
                "anthropic/claude-sonnet-4-5": {
                    "name": "Claude Sonnet 4.5",
                    "limit": {"context": 200000, "output": 64000},
                    "modalities": {"input": ["text", "image"]},
                    "reasoning": true
                },
                "gpt-5": {"reasoning": true}
            }
        },
        "moonshot": {
            "models": {
                "claude-sonnet-4-5": {"limit": {"context": 80000}}
            }
        },
        "zhipu": {
            "models": {
                "claude-sonnet-4-5": {
                    "limit": {"context": 100000},
                    "reasoning_options": [{"values": ["low", "medium", "high"]}]
                }
            }
        }
    });
    parse_modelsdev_catalog(&data)
}

#[test]
fn matching_prefers_same_provider_then_falls_back_to_search() -> Result<()> {
    let catalog = fixture_catalog();

    // 同名 provider 下以完整 id 命中。
    let exact = match_modelsdev_in_catalog(&catalog, "openrouter", "anthropic/claude-sonnet-4-5");
    assert_eq!(exact.status, ModelsDevMatchStatus::Exact);
    let meta = exact.meta.expect("exact match has meta");
    assert_eq!(meta.context_window, Some(200_000));
    assert_eq!(meta.max_output_tokens, Some(64_000));
    assert_eq!(meta.supports_images, Some(true));
    assert_eq!(meta.reasoning, Some(true));

    // 同名 provider 未命中 → 全库搜索出多候选（按 provider 名排序）。
    let multi = match_modelsdev_in_catalog(&catalog, "unknown", "claude-sonnet-4-5");
    assert_eq!(multi.status, ModelsDevMatchStatus::Candidates);
    assert_eq!(multi.candidates.len(), 2);
    assert_eq!(multi.candidates[0].provider, "moonshot");
    assert_eq!(multi.candidates[1].provider, "zhipu");

    // 全库未命中。
    let none = match_modelsdev_in_catalog(&catalog, "openrouter", "no-such-model");
    assert_eq!(none.status, ModelsDevMatchStatus::NotFound);
    Ok(())
}

#[test]
fn reasoning_options_values_are_normalized() -> Result<()> {
    let catalog = fixture_catalog();
    let multi = match_modelsdev_in_catalog(&catalog, "zhipu", "claude-sonnet-4-5");
    // zhipu 下唯一命中 → 归为精确匹配。
    assert_eq!(multi.status, ModelsDevMatchStatus::Exact);
    let meta = multi.meta.expect("meta");
    assert_eq!(
        meta.reasoning_levels,
        Some(vec![
            ReasoningLevel::Low,
            ReasoningLevel::Medium,
            ReasoningLevel::High
        ])
    );
    Ok(())
}

// 直连拉取（新增平台未落盘形态）：请求必须打向 {base_url}/models、
// OpenAI 协议用 Bearer 头，返回 OpenAI data 形态可正常解析。
#[test]
fn fetch_direct_uses_base_url_models_and_bearer_header() -> Result<()> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request_line = String::new();
        reader.read_line(&mut request_line).unwrap();
        let mut authorization = String::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap() == 0 || line.trim().is_empty() {
                break;
            }
            if line.to_ascii_lowercase().starts_with("authorization:") {
                authorization = line.trim().to_string();
            }
        }
        let body = r#"{"data":[{"id":"deepseek-chat"},{"id":"deepseek-reasoner"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let mut stream = stream;
        stream.write_all(response.as_bytes()).unwrap();
        (request_line, authorization)
    });

    let result = crate::providers::commands::fetch_provider_models_direct_inner(
        &crate::providers::types::ProviderProtocol::OpenaiChatCompletions,
        &format!("http://127.0.0.1:{port}/v1"),
        "sk-probe",
    )?;

    let (request_line, authorization) = server.join().unwrap();
    assert!(request_line.starts_with("GET /v1/models "));
    // reqwest 的 HeaderName 统一小写发送，header 名比较不区分大小写。
    assert_eq!(
        authorization.to_ascii_lowercase(),
        "authorization: bearer sk-probe"
    );
    assert_eq!(result.url, format!("http://127.0.0.1:{port}/v1/models"));
    assert_eq!(
        result
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["deepseek-chat", "deepseek-reasoner"]
    );
    Ok(())
}

// Anthropic 协议的 base_url 按官方约定不含版本段（…/api/anthropic），
// models 端点在 {base}/v1/models；已带 /v1 的填法不重复拼接。
#[test]
fn fetch_direct_anthropic_resolves_models_path() -> Result<()> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    fn serve_request_line() -> (u16, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            let body = r#"{"data":[{"id":"claude-x"}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let mut stream = stream;
            stream.write_all(response.as_bytes()).unwrap();
            request_line
        });
        (port, server)
    }

    // base_url 不带 /v1：拼 {base}/v1/models。
    let (port, server) = serve_request_line();
    crate::providers::commands::fetch_provider_models_direct_inner(
        &crate::providers::types::ProviderProtocol::AnthropicMessages,
        &format!("http://127.0.0.1:{port}/anthropic"),
        "sk-probe",
    )?;
    assert!(
        server
            .join()
            .unwrap()
            .starts_with("GET /anthropic/v1/models ")
    );

    // base_url 已带 /v1：不重复拼。
    let (port, server) = serve_request_line();
    crate::providers::commands::fetch_provider_models_direct_inner(
        &crate::providers::types::ProviderProtocol::AnthropicMessages,
        &format!("http://127.0.0.1:{port}/anthropic/v1"),
        "sk-probe",
    )?;
    assert!(
        server
            .join()
            .unwrap()
            .starts_with("GET /anthropic/v1/models ")
    );
    Ok(())
}

// /v1/models 返回 404 时回退 {base}/models 一次（DeepSeek 兼容层形态）。
#[test]
fn fetch_direct_anthropic_falls_back_on_404() -> Result<()> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let server = std::thread::spawn(move || {
        let mut request_lines = Vec::new();
        for not_found in [true, false] {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 || line.trim().is_empty() {
                    break;
                }
            }
            let response = if not_found {
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_string()
            } else {
                let body = r#"{"data":[{"id":"deepseek-chat"}]}"#;
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            let mut stream = stream;
            stream.write_all(response.as_bytes()).unwrap();
            request_lines.push(request_line);
        }
        request_lines
    });

    let result = crate::providers::commands::fetch_provider_models_direct_inner(
        &crate::providers::types::ProviderProtocol::AnthropicMessages,
        &format!("http://127.0.0.1:{port}/anthropic"),
        "sk-probe",
    )?;

    let request_lines = server.join().unwrap();
    assert!(request_lines[0].starts_with("GET /anthropic/v1/models "));
    assert!(request_lines[1].starts_with("GET /anthropic/models "));
    assert_eq!(
        result.url,
        format!("http://127.0.0.1:{port}/anthropic/models")
    );
    assert_eq!(
        result
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["deepseek-chat"]
    );
    Ok(())
}

// 前两跳都 404 且 base 挂在 /anthropic 子路径时，回退到根路径 /models
// （OpenAI 形态，Bearer 头）——DeepSeek 兼容层的实际布局。
#[test]
fn fetch_direct_anthropic_falls_back_to_root_models() -> Result<()> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let server = std::thread::spawn(move || {
        let mut request_lines = Vec::new();
        for not_found in [true, true, false] {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 || line.trim().is_empty() {
                    break;
                }
            }
            let response = if not_found {
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_string()
            } else {
                let body = r#"{"object":"list","data":[{"id":"deepseek-chat"}]}"#;
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            let mut stream = stream;
            stream.write_all(response.as_bytes()).unwrap();
            request_lines.push(request_line);
        }
        request_lines
    });

    let result = crate::providers::commands::fetch_provider_models_direct_inner(
        &crate::providers::types::ProviderProtocol::AnthropicMessages,
        &format!("http://127.0.0.1:{port}/anthropic"),
        "sk-probe",
    )?;

    let request_lines = server.join().unwrap();
    assert!(request_lines[0].starts_with("GET /anthropic/v1/models "));
    assert!(request_lines[1].starts_with("GET /anthropic/models "));
    assert!(request_lines[2].starts_with("GET /models "));
    assert_eq!(result.url, format!("http://127.0.0.1:{port}/models"));
    assert_eq!(
        result
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["deepseek-chat"]
    );
    Ok(())
}
