//! Console I/O for the `grf.io` library.
//!
//! Graphite `str` arrives as an owned Rust `String`. These functions do not
//! format arbitrary Graphite values. The compiler inserts `auto str` conversion
//! before the call.
//!
//! # Errors
//!
//! Write failures and end-of-file on input are returned as [`std::io::Error`].
//! They are not discarded. How a Graphite program observes those failures is
//! still an open decision in LLD-03. This Rust API does not invent that syntax.
//!
//! `input` writes the prompt, reads one line, and removes a single trailing
//! `\n`, `\r\n`, or `\r`. A zero-length read is `ErrorKind::UnexpectedEof`.

use std::io::{self, BufRead, Write};

pub fn print(value: String) -> io::Result<()> {
    let mut out = io::stdout().lock();
    write!(out, "{value}")?;
    out.flush()
}

pub fn println(value: String) -> io::Result<()> {
    let mut out = io::stdout().lock();
    writeln!(out, "{value}")?;
    out.flush()
}

pub fn input(prompt: String) -> io::Result<String> {
    let mut out = io::stdout().lock();
    write!(out, "{prompt}")?;
    out.flush()?;
    drop(out);

    let mut line = String::new();
    let read = io::stdin().lock().read_line(&mut line)?;
    if read == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "unexpected end of input",
        ));
    }
    Ok(without_line_ending(&line).to_string())
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
    fn signatures_take_owned_strings() {
        let _print: fn(String) -> io::Result<()> = print;
        let _println: fn(String) -> io::Result<()> = println;
        let _input: fn(String) -> io::Result<String> = input;
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
}
