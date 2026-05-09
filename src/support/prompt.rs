use crate::error::{ResipError, ResipResult};
use std::io::{self, Write};

pub fn prompt_required(label: &str) -> ResipResult<String> {
    loop {
        print!("{label}: ");
        io::stdout().flush().map_err(ResipError::FlushStdout)?;
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(ResipError::ReadStdin)?;
        let value = input.trim();
        if !value.is_empty() {
            return Ok(value.to_string());
        }
        eprintln!("{label} is required.");
    }
}

pub fn prompt_default(label: &str, default: &str) -> ResipResult<String> {
    print!("{label} [{default}]: ");
    io::stdout().flush().map_err(ResipError::FlushStdout)?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(ResipError::ReadStdin)?;
    let value = input.trim();
    if value.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(value.to_string())
    }
}

pub fn prompt_yes_no(label: &str, default: bool) -> ResipResult<bool> {
    let suffix = if default { "Y/n" } else { "y/N" };
    loop {
        print!("{label} [{suffix}]: ");
        io::stdout().flush().map_err(ResipError::FlushStdout)?;
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(ResipError::ReadStdin)?;
        let value = input.trim().to_lowercase();
        if value.is_empty() {
            return Ok(default);
        }
        match value.as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => eprintln!("Please answer y or n."),
        }
    }
}
