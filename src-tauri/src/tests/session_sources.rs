use anyhow::Result;

use crate::session;
use crate::session::model::SourceApp;

// zcode/dsh 的「不支持删除」原因是来源注册表数据,文案直达前端错误提示,
// 这里钉住防止悄悄漂移;两个分支不触环境与文件,无需 TestEnvGuard。
#[test]
fn unsupported_deletion_reports_app_reason() -> Result<()> {
    let zcode_error =
        session::delete_session(SourceApp::Zcode, "unused").expect_err("zcode 删除应整体不支持");
    assert!(
        zcode_error
            .to_string()
            .contains("ZCode session deletion is unsupported")
    );

    let dsh_error =
        session::delete_session(SourceApp::Dsh, "unused").expect_err("dsh 删除应整体不支持");
    assert!(
        dsh_error
            .to_string()
            .contains("DSH session deletion is unsupported")
    );

    Ok(())
}

// 来源注册表是全部来源分发的唯一事实:必须恰好覆盖每个 SourceApp 变体
// 一次;顺序是可观察行为(detect 输出与 usage sources 都按该顺序产出),
// 等于历史逐来源 join 的顺序,不得随意重排。
#[test]
fn sources_registry_covers_every_source_app_once_in_order() {
    let listed: Vec<SourceApp> = session::sources::SOURCES
        .iter()
        .map(|spec| spec.app)
        .collect();

    let expected = [
        SourceApp::Codex,
        SourceApp::ClaudeCode,
        SourceApp::OpenCode,
        SourceApp::Pi,
        SourceApp::GrokBuild,
        SourceApp::Zcode,
        SourceApp::Dsh,
    ];

    assert_eq!(listed, expected);

    // spec() 的 match 必须与表同源:表按值持副本,这里校验每个变体都路由到
    // 描述自己的条目,漏登记或错位都先在这里红。
    for app in expected {
        assert_eq!(session::sources::spec(app).app, app);
    }
}
