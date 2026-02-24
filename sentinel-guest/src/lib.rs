//! # sentinel-guest — Security Auditor Agent
//!
//! A multi-file security auditor that runs inside the SENTINEL sandbox.
//! It discovers Rust source files, sends each to the LLM for security
//! analysis, and writes an aggregate AUDIT_REPORT.md — but only after
//! the user approves a HITL manifest.

wit_bindgen::generate!({
    path: "../wit/sentinel.wit",
    world: "sentinel-guest",
});

use sentinel::agent::capabilities::*;
use sentinel::agent::hitl::*;
use sentinel::agent::logging::*;
use sentinel::agent::reasoning::*;

struct Component;

impl Guest for Component {
    fn run(context_json: String) -> i32 {
        log(LogLevel::Info, "auditor", "═══ SENTINEL Security Auditor starting ═══");
        log(LogLevel::Info, "auditor", &format!("Received context JSON: {}", context_json));

        // ── Parse context JSON ──────────────────────────────────────────
        let (target_dir, task_prompt) = parse_context(&context_json);
        log(LogLevel::Info, "auditor", &format!("Target directory: {}", target_dir));
        log(LogLevel::Info, "auditor", &format!("Task: {}", task_prompt));

        // ──────────────────────────────────────────────────────────────────
        // PHASE 1: Discovery — list all files in the workspace
        // ──────────────────────────────────────────────────────────────────
        log(LogLevel::Info, "auditor", "[Phase 1] Discovering workspace files...");

        let read_token = match request_fs_read(&target_dir, "List workspace files for security audit") {
            CapabilityResult::Granted(t) => t,
            CapabilityResult::Denied(reason) => {
                log(LogLevel::Error, "auditor", &format!("Cannot read workspace: {}", reason));
                return 1;
            }
        };

        let all_entries = match fs_list_dir(&read_token.id, &target_dir) {
            Ok(entries) => entries,
            Err(e) => {
                log(LogLevel::Error, "auditor", &format!("Cannot list directory: {}", e));
                return 1;
            }
        };

        // Collect .rs files — also recurse into src/ directories of sub-crates
        let mut rs_files: Vec<String> = Vec::new();

        // Check top-level for any .rs files
        for entry in &all_entries {
            if entry.ends_with(".rs") {
                rs_files.push(entry.clone());
            }
        }

        // Check known sub-crate src/ directories
        // Discover src/ subdirectories dynamically
        let mut sub_dirs: Vec<String> = Vec::new();
        for entry in &all_entries {
            let sub_src = if target_dir == "." {
                format!("{}/src", entry)
            } else {
                format!("{}/{}/src", target_dir, entry)
            };
            sub_dirs.push(sub_src);
        }

        for sub_dir in sub_dirs.iter() {
            // Request a read token for the sub-directory
            let sub_token = match request_fs_read(sub_dir, &format!("List {} for security audit", sub_dir)) {
                CapabilityResult::Granted(t) => t,
                CapabilityResult::Denied(_) => continue,
            };

            match fs_list_dir(&sub_token.id, sub_dir) {
                Ok(entries) => {
                    for entry in entries {
                        if entry.ends_with(".rs") {
                            rs_files.push(format!("{}/{}", sub_dir, entry));
                        }
                    }
                }
                Err(_) => continue,
            }

            release_capability(&sub_token.id);
        }

        log(LogLevel::Info, "auditor", &format!("[Phase 1] Found {} Rust source files", rs_files.len()));
        for f in &rs_files {
            log(LogLevel::Debug, "auditor", &format!("  → {}", f));
        }

        if rs_files.is_empty() {
            log(LogLevel::Warn, "auditor", "No .rs files found — nothing to audit.");
            return 0;
        }

        // ──────────────────────────────────────────────────────────────────
        // PHASE 2 & 3: Analysis + Reasoning — read each file and audit it
        // ──────────────────────────────────────────────────────────────────
        log(LogLevel::Info, "auditor", "[Phase 2+3] Analyzing files with LLM...");

        let provider = get_provider_name();
        log(LogLevel::Info, "auditor", &format!("Using LLM provider: {}", provider));

        let mut findings: Vec<String> = Vec::new();
        let mut files_audited: u32 = 0;
        let mut total_issues: u32 = 0;

        let system_prompt = format!("\
You are a senior security auditor. Your task: {}

Analyze the provided source code and report:

1. **Security Vulnerabilities**: unsafe blocks, unchecked inputs, path traversal, injection risks.
2. **Logic Flaws**: race conditions, integer overflow, error handling gaps, panics in production paths.
3. **Best Practice Violations**: missing input validation, hardcoded secrets, insufficient logging.

Format your response as a concise bullet list. If the code is clean, say \"No issues found.\"
Do NOT explain what the code does — only report problems.", task_prompt);

        for file_path in &rs_files {
            log(LogLevel::Info, "auditor", &format!("  Auditing: {}", file_path));

            // Get a read token for this specific file
            let file_token = match request_fs_read(file_path, &format!("Read {} for security audit", file_path)) {
                CapabilityResult::Granted(t) => t,
                CapabilityResult::Denied(reason) => {
                    log(LogLevel::Warn, "auditor", &format!("  Skipped (denied): {} — {}", file_path, reason));
                    findings.push(format!("### {}\n\n⚠️ Skipped: access denied — {}\n", file_path, reason));
                    continue;
                }
            };

            // Read the file contents
            let content = match fs_read(&file_token.id, file_path) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(e) => {
                    log(LogLevel::Warn, "auditor", &format!("  Skipped (read error): {} — {}", file_path, e));
                    findings.push(format!("### {}\n\n⚠️ Skipped: read error — {}\n", file_path, e));
                    release_capability(&file_token.id);
                    continue;
                }
            };

            release_capability(&file_token.id);

            // Skip very small files (< 50 bytes, likely empty or just re-exports)
            if content.len() < 50 {
                log(LogLevel::Debug, "auditor", &format!("  Skipped (too small): {} ({} bytes)", file_path, content.len()));
                continue;
            }

            // Send to LLM for security analysis
            let messages = vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.clone(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: format!("Audit this file (`{}`):\n\n```rust\n{}\n```", file_path, content),
                },
            ];

            match complete(&messages, Some(1024), Some(0.3), None) {
                Ok(resp) => {
                    let has_issues = !resp.content.to_lowercase().contains("no issues found");
                    if has_issues {
                        total_issues += 1;
                    }
                    findings.push(format!(
                        "### {}\n\n{}\n\n*Model: {} | Tokens: {}*\n",
                        file_path,
                        resp.content.trim(),
                        resp.model,
                        resp.usage.total_tokens
                    ));
                    files_audited += 1;
                    log(LogLevel::Info, "auditor", &format!(
                        "  ✓ {} — {} (tokens: {})",
                        file_path,
                        if has_issues { "issues found" } else { "clean" },
                        resp.usage.total_tokens
                    ));
                }
                Err(e) => {
                    log(LogLevel::Error, "auditor", &format!("  LLM error for {}: {}", file_path, e));
                    findings.push(format!("### {}\n\n⚠️ LLM error: {}\n", file_path, e));
                }
            }
        }

        log(LogLevel::Info, "auditor", &format!(
            "[Phase 2+3] Complete — audited {} files, {} with potential issues",
            files_audited, total_issues
        ));

        // ──────────────────────────────────────────────────────────────────
        // PHASE 4: Reporting — build the Markdown report and write it
        // ──────────────────────────────────────────────────────────────────
        log(LogLevel::Info, "auditor", "[Phase 4] Building audit report...");

        let report = format!(
            "# 🔒 SENTINEL Security Audit Report\n\n\
             **Generated by**: SENTINEL Security Auditor Agent\n\
             **LLM Provider**: {}\n\
             **Files Audited**: {}\n\
             **Files with Issues**: {}\n\n\
             ---\n\n\
             ## Findings\n\n\
             {}\n\n\
             ---\n\n\
             *This report was generated autonomously by the SENTINEL agent framework.*\n\
             *All file access was capability-gated and write access was HITL-approved.*\n",
            provider,
            files_audited,
            total_issues,
            findings.join("\n---\n\n"),
        );

        // ──────────────────────────────────────────────────────────────────
        // HITL GATE: Submit a manifest before writing the report
        // ──────────────────────────────────────────────────────────────────
        log(LogLevel::Info, "auditor", "Requesting HITL approval to write AUDIT_REPORT.md...");

        let manifest = ExecutionManifest {
            id: "audit-report-write-001".to_string(),
            action_description: format!(
                "Write security audit report (AUDIT_REPORT.md) — {} files audited, {} potential issues found",
                files_audited, total_issues
            ),
            parameters_json: format!(
                r#"{{"file": "AUDIT_REPORT.md", "size_bytes": {}, "files_audited": {}, "issues_found": {}}}"#,
                report.len(), files_audited, total_issues
            ),
            risk: RiskLevel::High,
        };

        match submit_manifest(&manifest) {
            ApprovalResult::Approved(_approval) => {
                log(LogLevel::Info, "auditor", "✓ HITL approved — writing report");
            }
            ApprovalResult::Rejected(reason) => {
                log(LogLevel::Error, "auditor", &format!("✗ HITL rejected: {}", reason));
                log(LogLevel::Info, "auditor", "Report was NOT written. Audit findings are in the logs above.");
                return 1;
            }
            ApprovalResult::TimedOut => {
                log(LogLevel::Error, "auditor", "✗ HITL timed out — report was NOT written");
                return 1;
            }
        }

        // ── Write the report ─────────────────────────────────────────────
        let write_token = match request_fs_write("AUDIT_REPORT.md", "Write security audit report after HITL approval") {
            CapabilityResult::Granted(t) => t,
            CapabilityResult::Denied(reason) => {
                log(LogLevel::Error, "auditor", &format!("Cannot write report: {}", reason));
                return 1;
            }
        };

        match fs_write(&write_token.id, "AUDIT_REPORT.md", report.as_bytes()) {
            Ok(_) => {
                log(LogLevel::Info, "auditor", "✓ AUDIT_REPORT.md written successfully");
            }
            Err(e) => {
                log(LogLevel::Error, "auditor", &format!("Failed to write report: {}", e));
                release_capability(&write_token.id);
                return 1;
            }
        }

        release_capability(&write_token.id);
        release_capability(&read_token.id);

        log(LogLevel::Info, "auditor", "═══ SENTINEL Security Auditor complete ═══");
        0
    }

    fn handle_event(event_type: String, _payload_json: String) -> String {
        log(LogLevel::Info, "auditor", &format!("Event received: {}", event_type));
        String::new()
    }
}

/// Parse the context JSON received from the host using serde_json.
/// Expected format: {"target_directory": "...", "task_prompt": "..."}
fn parse_context(json: &str) -> (String, String) {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(json) {
        let target_dir = val.get("target_directory").and_then(|v| v.as_str()).unwrap_or(".").to_string();
        let task_prompt = val.get("task_prompt").and_then(|v| v.as_str())
            .unwrap_or("Audit this codebase for security vulnerabilities.").to_string();
        (target_dir, task_prompt)
    } else {
        log(LogLevel::Error, "auditor", "Failed to parse context JSON, using defaults.");
        (".".to_string(), "Audit this codebase for security vulnerabilities.".to_string())
    }
}

/// Minimal JSON string extractor (avoids pulling in full serde for guest size).
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    let key_pos = json.find(&pattern)?;
    let after_key = &json[key_pos + pattern.len()..];
    // Skip `: ` or `:`
    let colon_pos = after_key.find(':')?;
    let after_colon = after_key[colon_pos + 1..].trim_start();
    if !after_colon.starts_with('"') {
        return None;
    }
    let value_start = 1; // skip opening quote
    let value_str = &after_colon[value_start..];
    let end_quote = value_str.find('"')?;
    Some(value_str[..end_quote].to_string())
}

export!(Component);
