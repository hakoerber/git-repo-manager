use std::path::{Path, PathBuf};
use std::process;

use super::output::*;

pub fn path_as_string(path: &Path) -> String {
    path.to_path_buf().into_os_string().into_string().unwrap()
}

pub fn env_home() -> String {
    match std::env::var("HOME") {
        Ok(path) => path,
        Err(e) => {
            print_error(&format!("Unable to read HOME: {e}"));
            process::exit(1);
        }
    }
}

pub fn expand_path(path: &Path) -> PathBuf {
    let expanded_path = match shellexpand::full_with_context(
        &path_as_string(path),
        || Some(env_home()),
        |name| -> Result<Option<String>, &'static str> {
            match name {
                "HOME" => Ok(Some(env_home())),
                _ => Ok(None),
            }
        },
    ) {
        Ok(std::borrow::Cow::Borrowed(path)) => path.to_owned(),
        Ok(std::borrow::Cow::Owned(path)) => path,
        Err(e) => {
            print_error(&format!("Unable to expand root: {e}"));
            process::exit(1);
        }
    };

    Path::new(&expanded_path).to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        std::env::set_var("HOME", "/home/test");
    }

    #[test]
    fn check_expand_tilde() {
        setup();
        assert_eq!(
            expand_path(Path::new("~/file")),
            Path::new("/home/test/file")
        );
    }

    #[test]
    fn check_expand_invalid_tilde() {
        setup();
        assert_eq!(
            expand_path(Path::new("/home/~/file")),
            Path::new("/home/~/file")
        );
    }

    #[test]
    fn check_expand_home() {
        setup();
        assert_eq!(
            expand_path(Path::new("$HOME/file")),
            Path::new("/home/test/file")
        );
        assert_eq!(
            expand_path(Path::new("${HOME}/file")),
            Path::new("/home/test/file")
        );
    }
}
