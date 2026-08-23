use std::io::{self, Write};

#[derive(Debug, thiserror::Error)]
#[error("cancelled")]
pub struct ConfirmationError;

pub fn require_go<R, W>(mut reader: R, mut writer: W, plan: &str) -> Result<(), ConfirmationError>
where
    R: FnMut(&str) -> io::Result<String>,
    W: Write,
{
    let _ = writeln!(writer, "{}\nType go to continue.", plan.trim_end());
    let answer = reader("continue: ").map_err(|_| ConfirmationError)?;
    if answer.trim() == "go" {
        Ok(())
    } else {
        Err(ConfirmationError)
    }
}
