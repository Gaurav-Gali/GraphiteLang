//! Console I/O for the `grf.io` library.
//!
//! Graphite `str` arrives as an owned Rust `String`. These functions do not
//! format arbitrary Graphite values. The compiler inserts `auto str` conversion
//! before the call.
//!
//! Graphite v0.1 has no error-handling syntax. A failed read or write reports
//! the underlying I/O error and terminates the process with status 1. A
//! successful read of an empty line returns an empty string. End of file is
//! not reported as an empty string.

use std::io::{self, BufRead, Write};
use std::process;

pub fn print(value: String) {
    if let Err(error) = write_all(&value, false) {
        fail_io(error);
    }
}

pub fn println(value: String) {
    if let Err(error) = write_all(&value, true) {
        fail_io(error);
    }
}

pub fn input(prompt: String) -> String {
    if let Err(error) = write_all(&prompt, false) {
        fail_io(error);
    }
    match read_line(&mut io::stdin().lock()) {
        Ok(line) => line,
        Err(error) => fail_io(error),
    }
}

fn write_all(value: &str, newline: bool) -> io::Result<()> {
    let mut out = io::stdout().lock();
    if newline {
        writeln!(out, "{value}")?;
    } else {
        write!(out, "{value}")?;
    }
    out.flush()
}

fn read_line(input: &mut impl BufRead) -> io::Result<String> {
    let mut line = String::new();
    let read = input.read_line(&mut line)?;
    finish_line(read, &line)
}

/// Distinguishes a successful empty line from a failed read.
///
/// `bytes_read == 0` is end of file or a failed empty read. A successful read
/// of only a line ending returns an empty string.
fn finish_line(bytes_read: usize, line: &str) -> io::Result<String> {
    if bytes_read == 0 {
        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "unexpected end of input",
        ))
    } else {
        Ok(without_line_ending(line).to_string())
    }
}

fn fail_io(error: io::Error) -> ! {
    eprintln!("graphite: input/output error: {error}");
    process::exit(1);
}

fn without_line_ending(line: &str) -> &str {
    line.strip_suffix("\r\n")
        .or_else(|| line.strip_suffix('\n'))
        .or_else(|| line.strip_suffix('\r'))
        .unwrap_or(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_match_the_graphite_contract() {
        let _print: fn(String) = print;
        let _println: fn(String) = println;
        let _input: fn(String) -> String = input;
    }

    #[test]
    fn strips_one_line_ending() {
        assert_eq!(without_line_ending("hello\n"), "hello");
        assert_eq!(without_line_ending("hello\r\n"), "hello");
        assert_eq!(without_line_ending("hello\r"), "hello");
        assert_eq!(without_line_ending("hello"), "hello");
        assert_eq!(without_line_ending("hello\n\n"), "hello\n");
        assert_eq!(without_line_ending("hello \n"), "hello ");
    }

    #[test]
    fn successful_empty_line_is_not_an_io_failure() {
        let line = finish_line(1, "\n").expect("empty line");
        assert_eq!(line, "");
        let spaced = finish_line(2, "\r\n").expect("crlf");
        assert_eq!(spaced, "");
    }

    #[test]
    fn eof_is_an_error_and_not_an_empty_string() {
        let error = finish_line(0, "").expect_err("eof");
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        assert!(error.to_string().contains("unexpected end of input"));
    }
}
