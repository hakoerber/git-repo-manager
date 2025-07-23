use std::process;

use thiserror::Error;

#[derive(Clone)]
pub struct AuthToken(String);

#[derive(Error, Debug)]
pub enum Error {
    #[error("token command failed: {0}")]
    TokenCommandInvocationFailed(#[from] std::io::Error),

    #[error("token command failed: {0}")]
    TokenCommandInvocationInvalidUtf8(#[from] std::string::FromUtf8Error),

    #[error("token command failed, stderr: {0}")]
    TokenCommandFailed(String),

    #[error("token command produced stderr: {0}")]
    TokenCommandStderr(String),

    #[error("token command output empty")]
    TokenCommandEmptyOutput,

    #[error("token command output did not contain any newline")]
    TokenCommandNoNewlineInOutput,
}

impl AuthToken {
    pub fn access(&self) -> &str {
        &self.0
    }
}

pub fn get_token_from_command(command: &str) -> Result<AuthToken, Error> {
    let output = process::Command::new("/usr/bin/env")
        .arg("sh")
        .arg("-c")
        .arg(command)
        .output()?;

    let stderr = String::from_utf8(output.stderr)?;
    let stdout = String::from_utf8(output.stdout)?;

    if !output.status.success() {
        return Err(Error::TokenCommandFailed(stderr));
    }

    if !stderr.is_empty() {
        return Err(Error::TokenCommandStderr(stderr));
    }

    if stdout.is_empty() {
        return Err(Error::TokenCommandEmptyOutput);
    }

    let token = stdout
        .split('\n')
        .next()
        .ok_or(Error::TokenCommandNoNewlineInOutput)?;

    Ok(AuthToken(token.to_string()))
}
