use std::process::Output;

use anyhow::bail;

pub(crate) fn check_output(output: Output, operation: &str) -> anyhow::Result<Vec<u8>> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let diagnostic = if stderr.trim().is_empty() {
            stdout.trim_end()
        } else {
            stderr.trim_end()
        };

        if diagnostic.is_empty() {
            bail!(
                "{operation} failed ({}); no diagnostic output",
                output.status
            );
        }

        bail!("{operation} failed ({}): {diagnostic}", output.status);
    }

    Ok(output.stdout)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn reports_status_and_available_diagnostics() -> anyhow::Result<()> {
        for (script, diagnostic) in [
            ("echo stdout; echo stderr >&2; exit 7", "stderr"),
            ("echo stdout; exit 7", "stdout"),
            ("exit 7", "no diagnostic output"),
            ("kill -TERM $$", "signal"),
        ] {
            let output = Command::new("sh").args(["-c", script]).output()?;
            let status = output.status.to_string();
            let error = check_output(output, "test operation").unwrap_err();
            let message = error.to_string();
            assert!(message.contains("test operation"));
            assert!(message.contains(&status));
            assert!(message.contains(diagnostic));
        }

        let output = Command::new("sh").args(["-c", "printf success"]).output()?;
        assert_eq!(check_output(output, "test operation")?, b"success");

        Ok(())
    }
}
