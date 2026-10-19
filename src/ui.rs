//! Terminal output: coloured status lines.

use std::io::IsTerminal;

/// Wraps text in an ANSI style when the given stream is a terminal.
fn paint(code: &str, text: &str, terminal: bool) -> String {
    if terminal {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

/// Succeeds if stdout is a terminal.
pub fn stdout_is_terminal() -> bool {
    std::io::stdout().is_terminal()
}

/// Prints a success line.
pub fn ok(msg: impl AsRef<str>) {
    println!("{} {}", paint("32", "✓", stdout_is_terminal()), msg.as_ref());
}

/// Prints a neutral info line.
pub fn info(msg: impl AsRef<str>) {
    println!("{} {}", paint("2", "•", stdout_is_terminal()), msg.as_ref());
}

/// Prints a warning to stderr.
pub fn warn(msg: impl AsRef<str>) {
    eprintln!("{} {}", paint("33", "!", std::io::stderr().is_terminal()), msg.as_ref());
}

/// Prints an error to stderr.
pub fn error(msg: impl AsRef<str>) {
    eprintln!("{} {}", paint("31", "✗", std::io::stderr().is_terminal()), msg.as_ref());
}

/// Makes text bold on a terminal.
pub fn bold(text: &str) -> String {
    paint("1", text, stdout_is_terminal())
}

/// Makes text dim on a terminal.
pub fn dim(text: &str) -> String {
    paint("2", text, stdout_is_terminal())
}
