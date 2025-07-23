use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("found non-utf8 path: {:?}", .path)]
    NonUtf8 { path: PathBuf },
    #[error("failed getting env variable `{}`: {}", .variable, .error)]
    Env { variable: String, error: String },
    #[error("failed expanding path: {}", .error)]
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

pub fn env_home() -> Result<String, Error> {
    std::env::var("HOME").map_err(|e| Error::Env {
        variable: "HOME".to_owned(),
        error: e.to_string(),
    })
}

pub fn expand_path(path: &Path) -> Result<PathBuf, Error> {
    let home = env_home()?;
    let expanded_path = match shellexpand::full_with_context(
        &path_as_string(path)?,
        || Some(home),
        |name| -> Result<Option<String>, Error> {
            match name {
                "HOME" => Ok(Some(env_home()?)),
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
