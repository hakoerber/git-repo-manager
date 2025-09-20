#![expect(
    clippy::print_stdout,
    reason = "this module handles all console output"
)]

use std::fmt::Display;

use console::{Style, Term};

pub(super) fn print_repo_error(repo: &str, message: &str) {
    print_error(&format!("{repo}: {message}"));
}

pub(super) fn print_error(message: &str) {
    let stderr = Term::stderr();
    let mut style = Style::new().red();
    if stderr.is_term() {
        style = style.force_styling(true);
    }
    stderr
        .write_line(&format!("[{}] {}", style.apply_to('\u{2718}'), &message))
        .expect("failed writing to stderr");
}

pub(super) fn print_repo_action(repo: &str, message: &str) {
    print_action(&format!("{repo}: {message}"));
}

pub(super) fn print_action(message: &str) {
    let stdout = Term::stdout();
    let mut style = Style::new().yellow();
    if stdout.is_term() {
        style = style.force_styling(true);
    }
    stdout
        .write_line(&format!("[{}] {}", style.apply_to('\u{2699}'), &message))
        .expect("failed writing to stderr");
}

pub(super) fn print_warning(message: impl Display) {
    let stderr = Term::stderr();
    let mut style = Style::new().yellow();
    if stderr.is_term() {
        style = style.force_styling(true);
    }
    stderr
        .write_line(&format!("[{}] {}", style.apply_to('!'), &message))
        .expect("failed writing to stderr");
}

pub(super) fn print_repo_success(repo: &str, message: &str) {
    print_success(&format!("{repo}: {message}"));
}

pub(super) fn print_success(message: &str) {
    let stdout = Term::stdout();
    let mut style = Style::new().green();
    if stdout.is_term() {
        style = style.force_styling(true);
    }

    stdout
        .write_line(&format!("[{}] {}", style.apply_to('\u{2714}'), &message))
        .expect("failed writing to stderr");
}

pub(super) fn println(message: &str) {
    println!("{message}");
}

pub(super) fn print(message: &str) {
    print!("{message}");
}
