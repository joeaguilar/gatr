use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::parser::{strip_ansi, ParseOutcome, StreamParser};

/// Exit code reported when the wrapped command is killed by `--timeout`,
/// mirroring coreutils `timeout(1)`.
pub const TIMEOUT_EXIT: i32 = 124;

pub struct RunResult {
    pub exit: i32,
    pub timed_out: bool,
    pub dur_s: f64,
    pub parse: ParseOutcome,
}

/// Run `cmd`, streaming merged stdout+stderr (in arrival order, via a single
/// shared pipe) to the log file while feeding the stream parser. Returns the
/// wrapped command's exit code for passthrough.
pub fn run_command(
    cmd: &[String],
    log_path: &Path,
    parser: StreamParser,
    timeout: Option<Duration>,
) -> Result<RunResult> {
    let (reader, writer) = os_pipe::pipe().context("failed to create pipe")?;
    let writer2 = writer.try_clone().context("failed to clone pipe writer")?;

    let mut command = Command::new(&cmd[0]);
    command
        .args(&cmd[1..])
        .stdin(Stdio::inherit())
        .stdout(writer)
        .stderr(writer2);
    #[cfg(unix)]
    if timeout.is_some() {
        // Own process group so a timeout kill reaches grandchildren too
        // (otherwise they keep the pipe open and the reader never sees EOF).
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
    }

    let start = Instant::now();
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to run '{}'", cmd[0]))?;
    // Drop the Command so our copies of the pipe writers close; the reader
    // then gets EOF as soon as the child (group) exits.
    drop(command);

    let log_file = File::create(log_path)
        .with_context(|| format!("failed to create log file {}", log_path.display()))?;

    let reader_thread = std::thread::spawn(move || -> StreamParser {
        let mut reader = BufReader::new(reader);
        let mut writer = BufWriter::new(log_file);
        let mut parser = parser;
        let mut buf: Vec<u8> = Vec::with_capacity(8192);
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let _ = writer.write_all(&buf); // raw bytes as received
                    let text = String::from_utf8_lossy(&buf);
                    parser.feed_line(&strip_ansi(text.trim_end_matches(['\n', '\r'])));
                }
            }
        }
        let _ = writer.flush();
        parser
    });

    let deadline = timeout.map(|t| start + t);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().context("failed to wait for child")? {
            break status;
        }
        if let Some(d) = deadline {
            if !timed_out && Instant::now() >= d {
                timed_out = true;
                kill_child(&mut child);
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let dur_s = start.elapsed().as_secs_f64();

    let parser = reader_thread
        .join()
        .map_err(|_| anyhow::anyhow!("log reader thread panicked"))?;

    let exit = if timed_out {
        TIMEOUT_EXIT
    } else {
        exit_code(status)
    };
    Ok(RunResult {
        exit,
        timed_out,
        dur_s,
        parse: parser.finish(),
    })
}

fn kill_child(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // Negative pid = the whole process group we created at spawn.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

fn exit_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return 128 + sig;
        }
    }
    1
}

/// Parse `10m`, `90s`, `1h30m`, or a bare number of seconds.
pub fn parse_timeout(s: &str) -> Result<Duration> {
    let s = s.trim();
    if s.is_empty() {
        bail!("empty timeout");
    }
    if let Ok(secs) = s.parse::<u64>() {
        return Ok(Duration::from_secs(secs));
    }
    let mut total = 0.0f64;
    let mut num = String::new();
    let mut any_unit = false;
    for c in s.chars() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
        } else {
            let value: f64 = num
                .parse()
                .with_context(|| format!("invalid timeout '{s}'"))?;
            num.clear();
            let mult = match c {
                's' => 1.0,
                'm' => 60.0,
                'h' => 3600.0,
                _ => bail!("invalid timeout unit '{c}' in '{s}' (use s, m, h)"),
            };
            total += value * mult;
            any_unit = true;
        }
    }
    if !num.is_empty() || !any_unit {
        bail!("invalid timeout '{s}' (examples: 90, 90s, 10m, 1h30m)");
    }
    Ok(Duration::from_secs_f64(total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(unknown_lints, clippy::duration_suboptimal_units)]
    fn timeout_formats() {
        assert_eq!(parse_timeout("90").unwrap(), Duration::from_secs(90));
        assert_eq!(parse_timeout("90s").unwrap(), Duration::from_secs(90));
        assert_eq!(parse_timeout("10m").unwrap(), Duration::from_secs(600));
        assert_eq!(parse_timeout("1h30m").unwrap(), Duration::from_secs(5400));
        assert!(parse_timeout("10x").is_err());
        assert!(parse_timeout("").is_err());
        assert!(parse_timeout("10m5").is_err());
    }
}
