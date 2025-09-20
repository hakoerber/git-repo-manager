use std::{
    fmt,
    path::{Path, PathBuf},
};

use thiserror::Error;

#[derive(Debug)]
pub struct EnvVariableName(String);

impl fmt::Display for EnvVariableName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Found non-utf8 path: {:?}", .path)]
    NonUtf8 { path: PathBuf },
    #[error("Failed getting env variable `{}`: {}", .variable, .error)]
    Env {
        variable: EnvVariableName,
        error: String,
    },
    #[error("Failed expanding path: {}", .error)]
    Expand { error: String },
}

pub fn path_as_string(path: &Path) -> Result<String, Error> {
    path.to_path_buf()
        .into_os_string()
        .into_string()
        .map_err(|_s| Error::NonUtf8 {
            path: path.to_path_buf(),
        })
}

pub fn env_home() -> Result<PathBuf, Error> {
    Ok(PathBuf::from(std::env::var("HOME").map_err(|e| {
        Error::Env {
            variable: EnvVariableName("HOME".to_owned()),
            error: e.to_string(),
        }
    })?))
}

pub fn expand_path(path: &Path) -> Result<PathBuf, Error> {
    let home = path_as_string(&env_home()?)?;
    let expanded_path = match shellexpand::full_with_context(
        &path_as_string(path)?,
        || Some(home.clone()),
        |name| -> Result<Option<String>, Error> {
            match name {
                "HOME" => Ok(Some(home.clone())),
                _ => Ok(None),
            }
        },
    ) {
        Ok(std::borrow::Cow::Borrowed(path)) => path.to_owned(),
        Ok(std::borrow::Cow::Owned(path)) => path,
        Err(e) => {
            return Err(Error::Expand {
                error: e.cause.to_string(),
            });
        }
    };

    Ok(Path::new(&expanded_path).to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_expand_tilde() -> Result<(), Error> {
        temp_env::with_var("HOME", Some("/home/test"), || {
            assert_eq!(
                expand_path(Path::new("~/file"))?,
                Path::new("/home/test/file")
            );
            Ok(())
        })
    }

    #[test]
    fn check_expand_invalid_tilde() -> Result<(), Error> {
        temp_env::with_var("HOME", Some("/home/test"), || {
            assert_eq!(
                expand_path(Path::new("/home/~/file"))?,
                Path::new("/home/~/file")
            );
            Ok(())
        })
    }

    #[test]
    fn check_expand_home() -> Result<(), Error> {
        temp_env::with_var("HOME", Some("/home/test"), || {
            assert_eq!(
                expand_path(Path::new("$HOME/file"))?,
                Path::new("/home/test/file")
            );
            assert_eq!(
                expand_path(Path::new("${HOME}/file"))?,
                Path::new("/home/test/file")
            );
            Ok(())
        })
    }
}
