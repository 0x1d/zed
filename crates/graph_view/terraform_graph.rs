//! Runs `terraform graph` for dependency DOT output.
//!
//! Manual verification: run from a directory after `terraform init` with Terraform on `PATH`.
//! Subprocess behavior is covered manually; pure helpers are unit-tested below.

use std::path::Path;
use std::process::Output;

use anyhow::Context;
use util::command::new_command;

const STDERR_CAP_BYTES: usize = 4 * 1024;

fn cap_trimmed_stderr(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim();
    if trimmed.len() <= STDERR_CAP_BYTES {
        return trimmed.to_string();
    }
    let mut end = STDERR_CAP_BYTES;
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &trimmed[..end])
}

fn stderr_suggests_unsupported_plan_type_flag(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("unknown flag")
        || lower.contains("unrecognized flag")
        || lower.contains("flag provided but not defined")
        || lower.contains("invalid flag")
        || (lower.contains("-type") && (lower.contains("unknown") || lower.contains("invalid")))
        || (lower.contains("type") && lower.contains("unsupported"))
}

async fn terraform_graph_output(cwd: &Path, with_plan_type: bool) -> std::io::Result<Output> {
    let mut command = new_command("terraform");
    command.current_dir(cwd);
    if with_plan_type {
        command.args(["graph", "-type=plan"]);
    } else {
        command.args(["graph"]);
    }
    command.output().await
}

fn stdout_utf8(output: Output) -> anyhow::Result<String> {
    String::from_utf8(output.stdout).context("terraform graph stdout was not valid UTF-8")
}

/// Runs `terraform graph` in `cwd` and returns DOT text from stdout.
///
/// Tries `terraform graph -type=plan` first; if that fails with an error that suggests the
/// `-type=plan` flag is not supported, retries with plain `terraform graph`.
pub async fn run_terraform_graph(cwd: &Path) -> anyhow::Result<String> {
    let plan_try = terraform_graph_output(cwd, true)
        .await
        .context("failed to spawn terraform graph -type=plan")?;

    if plan_try.status.success() {
        return stdout_utf8(plan_try);
    }

    let plan_stderr = String::from_utf8_lossy(&plan_try.stderr);
    if stderr_suggests_unsupported_plan_type_flag(plan_stderr.as_ref()) {
        let fallback = terraform_graph_output(cwd, false)
            .await
            .context("failed to spawn terraform graph")?;

        if fallback.status.success() {
            return stdout_utf8(fallback);
        }

        let message = cap_trimmed_stderr(&fallback.stderr);
        anyhow::bail!("terraform graph failed: {message}");
    }

    let message = cap_trimmed_stderr(&plan_try.stderr);
    anyhow::bail!("terraform graph -type=plan failed: {message}");
}

#[cfg(test)]
mod tests {
    use super::{cap_trimmed_stderr, stderr_suggests_unsupported_plan_type_flag};

    #[test]
    fn detects_unknown_flag_and_type_hints() {
        assert!(stderr_suggests_unsupported_plan_type_flag(
            "Error: unknown flag -type"
        ));
        assert!(stderr_suggests_unsupported_plan_type_flag(
            "FLAG PROVIDED BUT NOT DEFINED: -type=plan"
        ));
        assert!(stderr_suggests_unsupported_plan_type_flag(
            "unsupported graph type for this terraform version"
        ));
        assert!(!stderr_suggests_unsupported_plan_type_flag(
            "Error: no configuration files"
        ));
    }

    #[test]
    fn caps_stderr_at_4kib() {
        let long = "x".repeat(5000);
        let capped = cap_trimmed_stderr(long.as_bytes());
        assert!(capped.len() <= 4 * 1024 + 4);
        assert!(capped.ends_with('…'));
    }
}
