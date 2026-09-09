use anyhow::{Context, Result, bail};
use uuid::Uuid;

use super::{ImportLevel, ImportPreview, ImportResult, SessionDetail, SourceApp};

pub(crate) fn generate_target_session_id(target_app: SourceApp) -> String {
    match target_app {
        SourceApp::OpenCode => format!("ses_{}", Uuid::new_v4().simple()),
        SourceApp::Codex | SourceApp::ClaudeCode | SourceApp::Pi => Uuid::new_v4().to_string(),
    }
}

pub(crate) fn preview_import_inner(
    source_app: SourceApp,
    source_session_id: &str,
    target_app: SourceApp,
    transcript_path: Option<&str>,
) -> Result<ImportPreview> {
    let detail = super::get_session_inner(source_app, source_session_id, transcript_path)?;
    let assessment = assess_import(&detail, target_app);
    let placeholder_session_id = "<generated-session-id>";

    Ok(ImportPreview {
        source_app,
        source_session_id: source_session_id.to_string(),
        target_app,
        supported: assessment.supported,
        import_level: assessment.import_level,
        warnings: assessment.warnings,
        created_paths: super::exporter(target_app)
            .planned_import_paths(&detail.summary, placeholder_session_id)?,
        backup_paths: Vec::new(),
    })
}

pub(crate) fn import_session_inner(
    source_app: SourceApp,
    source_session_id: &str,
    target_app: SourceApp,
    transcript_path: Option<&str>,
) -> Result<ImportResult> {
    let detail = super::get_session_inner(source_app, source_session_id, transcript_path)?;
    let assessment = assess_import(&detail, target_app);

    if !assessment.supported {
        bail!(
            "Import from {} to {} is not supported in the current MVP.",
            source_app.as_str(),
            target_app.as_str()
        );
    }

    let new_session_id = generate_target_session_id(target_app);
    let (created_session_id, created_paths) =
        super::exporter(target_app).export_session(&detail, &new_session_id)?;

    super::get_session_inner(target_app, &created_session_id, None).with_context(|| {
        format!(
            "Imported session was written, but read-back validation failed for {}.",
            created_session_id
        )
    })?;

    Ok(ImportResult {
        target_app,
        created_session_id,
        created_paths,
        backup_paths: Vec::new(),
        resume_cwd: detail.summary.cwd.clone(),
        warnings: assessment.warnings,
    })
}

struct ImportAssessment {
    supported: bool,
    import_level: ImportLevel,
    warnings: Vec<String>,
}

fn assess_import(detail: &SessionDetail, target_app: SourceApp) -> ImportAssessment {
    let source_app = detail.summary.source_app;

    if detail.messages.is_empty() {
        return ImportAssessment {
            supported: false,
            import_level: ImportLevel::Unsupported,
            warnings: vec!["The source session has no importable messages.".to_string()],
        };
    }

    if source_app == target_app {
        return ImportAssessment {
            supported: false,
            import_level: ImportLevel::Unsupported,
            warnings: vec![
                "The current MVP only exposes cross-program import. Same-app cloning is intentionally disabled."
                    .to_string(),
            ],
        };
    }

    let mut warnings = vec![
        "Only the normalized message timeline is imported. Side-channel runtime events are skipped."
            .to_string(),
        "The target program may render the imported conversation differently from the original UI."
            .to_string(),
        "This import path creates a brand-new session file, so backup paths are usually empty."
            .to_string(),
    ];

    if detail.summary.cwd.is_none() {
        warnings.push(
            "The source session does not expose a cwd, so the importer will fall back to the home directory."
                .to_string(),
        );
    }

    if !detail.events.is_empty() {
        warnings.push("Non-message events are not recreated in the target program.".to_string());
    }

    match (source_app, target_app) {
        (SourceApp::Codex | SourceApp::OpenCode | SourceApp::Pi, SourceApp::ClaudeCode) => {
            warnings.push(
                "Tool calls are mapped into Claude tool_use/tool_result records on a best-effort basis."
                    .to_string(),
            )
        }
        (SourceApp::ClaudeCode | SourceApp::OpenCode | SourceApp::Pi, SourceApp::Codex) => {
            warnings.push(
                "Codex import writes transcript JSONL only. Reins validates the written transcript, but Codex still needs to resume the session once before its internal SQLite state is populated."
                    .to_string(),
            )
        }
        (_, SourceApp::OpenCode) => warnings.push(
            "OpenCode import is delegated to the official CLI importer using generated session JSON."
                .to_string(),
        ),
        (_, SourceApp::Pi) => warnings.push(
            "Pi import writes a v3 JSONL session under the cwd-specific Pi session directory."
                .to_string(),
        ),
        _ => {}
    }

    ImportAssessment {
        supported: true,
        import_level: ImportLevel::Partial,
        warnings,
    }
}
