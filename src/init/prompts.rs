//! Prompt helpers — single interactivity gate, input functions.

use std::io::{self, BufRead, Write};
use crate::error::{Error, Result};

/// Single interactivity gate (gh CLI pattern).
/// Returns true if we can prompt the user.
pub fn can_prompt() -> bool {
    is_tty()
}

pub fn confirm(prompt: &str, default: bool) -> Result<bool> {
    let hint = if default { "[Y/n]" } else { "[y/N]" };
    eprint!("{prompt} {hint}: ");
    io::stderr().flush().ok();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).map_err(Error::Io)?;
    let answer = line.trim().to_lowercase();
    if answer.is_empty() {
        Ok(default)
    } else {
        Ok(answer == "y" || answer == "yes")
    }
}

pub fn select(prompt: &str, default: u32, options: &[(&str, &str)]) -> Result<u32> {
    for (i, (label, desc)) in options.iter().enumerate() {
        eprintln!("    {}. {label} — {desc}", i + 1);
    }
    eprintln!();
    eprint!("{prompt}: ");
    io::stderr().flush().ok();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).map_err(Error::Io)?;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(default);
    }
    match trimmed.parse::<u32>() {
        Ok(n) if n >= 1 && n <= options.len() as u32 => Ok(n),
        _ => {
            eprintln!("  Invalid choice, using default");
            Ok(default)
        }
    }
}

pub fn input(prompt: &str) -> Result<String> {
    eprint!("  {prompt}: ");
    io::stderr().flush().ok();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).map_err(Error::Io)?;
    Ok(line.trim().to_string())
}

pub fn input_default(prompt: &str, default: &str) -> Result<String> {
    eprint!("  {prompt} [{default}]: ");
    io::stderr().flush().ok();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).map_err(Error::Io)?;
    let value = line.trim();
    if value.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(value.to_string())
    }
}

pub fn password(prompt: &str) -> Result<String> {
    eprint!("  {prompt}: ");
    io::stderr().flush().ok();
    // In a real implementation we'd disable echo. For now, just read.
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).map_err(Error::Io)?;
    Ok(line.trim().to_string())
}

pub fn ask_yn(label: &str, default: bool) -> Result<bool> {
    let hint = if default { "[Y/n]" } else { "[y/N]" };
    eprint!("  {label} {hint}: ");
    io::stderr().flush().ok();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).map_err(Error::Io)?;
    let answer = line.trim().to_lowercase();
    if answer.is_empty() {
        Ok(default)
    } else {
        Ok(answer == "y" || answer == "yes")
    }
}

#[cfg(unix)]
fn is_tty() -> bool {
    unsafe { libc_isatty(0) != 0 }
}

#[cfg(unix)]
unsafe fn libc_isatty(fd: i32) -> i32 {
    unsafe extern "C" {
        safe fn isatty(fd: i32) -> i32;
    }
    isatty(fd)
}

#[cfg(not(unix))]
fn is_tty() -> bool {
    false
}
