use std::fmt::Display;

use console::{Style, Term};

pub fn print_repo_error(repo: &str, message: &str) {
    print_error(&format!("{repo}: {message}"));
}

pub fn print_error(message: &str) {
    let stderr = Term::stderr();
    let mut style = Style::new().red();
    if stderr.is_term() {
        style = style.force_styling(true);
    }
    stderr
        .write_line(&format!("[{}] {}", style.apply_to('\u{2718}'), &message))
        .unwrap();
}

pub fn print_repo_action(repo: &str, message: &str) {
    print_action(&format!("{repo}: {message}"));
}

pub fn print_action(message: &str) {
    let stdout = Term::stdout();
    let mut style = Style::new().yellow();
    if stdout.is_term() {
        style = style.force_styling(true);
    }
    stdout
        .write_line(&format!("[{}] {}", style.apply_to('\u{2699}'), &message))
        .unwrap();
}

pub fn print_warning(message: impl Display) {
    let stderr = Term::stderr();
    let mut style = Style::new().yellow();
    if stderr.is_term() {
        style = style.force_styling(true);
    }
    stderr
        .write_line(&format!("[{}] {}", style.apply_to('!'), &message))
        .unwrap();
}

pub fn print_repo_success(repo: &str, message: &str) {
    print_success(&format!("{repo}: {message}"));
}

pub fn print_success(message: &str) {
    let stdout = Term::stdout();
    let mut style = Style::new().green();
    if stdout.is_term() {
        style = style.force_styling(true);
    }

    stdout
        .write_line(&format!("[{}] {}", style.apply_to('\u{2714}'), &message))
        .unwrap();
}
