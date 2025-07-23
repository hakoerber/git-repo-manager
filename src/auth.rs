use std::process;

#[derive(Clone)]
pub struct AuthToken(String);

impl AuthToken {
    pub fn access(&self) -> &str {
        &self.0
    }
}

pub fn get_token_from_command(command: &str) -> Result<AuthToken, String> {
    let output = process::Command::new("/usr/bin/env")
        .arg("sh")
        .arg("-c")
        .arg(command)
        .output()
        .map_err(|error| format!("Failed to run token-command: {error}"))?;

    let stderr = String::from_utf8(output.stderr).map_err(|error| error.to_string())?;
    let stdout = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;

    if !output.status.success() {
        if !stderr.is_empty() {
            return Err(format!("Token command failed: {stderr}"));
        } else {
            Err(String::from("Token command failed."))
        };
    }

    if !stderr.is_empty() {
        return Err(format!("Token command produced stderr: {stderr}"));
    }

    if stdout.is_empty() {
        return Err(String::from("Token command did not produce output"));
    }

    let token = stdout
        .split('\n')
        .next()
        .ok_or_else(|| String::from("Output did not contain any newline"))?;

    Ok(AuthToken(token.to_string()))
}
