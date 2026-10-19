use std::io::IsTerminal;

fn paint(code: &str, text: &str, terminal: bool) -> String {
    if terminal {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn stdout_is_terminal() -> bool {
    std::io::stdout().is_terminal()
}

pub fn ok(msg: impl AsRef<str>) {
    println!("{} {}", paint("32", "✓", stdout_is_terminal()), msg.as_ref());
}

pub fn info(msg: impl AsRef<str>) {
    println!("{} {}", paint("2", "•", stdout_is_terminal()), msg.as_ref());
}

pub fn warn(msg: impl AsRef<str>) {
    eprintln!("{} {}", paint("33", "!", std::io::stderr().is_terminal()), msg.as_ref());
}

pub fn error(msg: impl AsRef<str>) {
    eprintln!("{} {}", paint("31", "✗", std::io::stderr().is_terminal()), msg.as_ref());
}

pub fn bold(text: &str) -> String {
    paint("1", text, stdout_is_terminal())
}

pub fn dim(text: &str) -> String {
    paint("2", text, stdout_is_terminal())
}
