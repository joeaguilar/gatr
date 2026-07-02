use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

/// Self-update: pull the source checkout, rebuild, replace the running
/// binary. Mirrors `itr upgrade` / `kgr upgrade`. The source location is
/// baked in at build time (`GATR_SOURCE_DIR`), overridable via the env var —
/// binaries installed from a release archive should update with `install.sh`.
pub fn cmd_upgrade() -> Result<i32> {
    let src_dir = std::env::var("GATR_SOURCE_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| env!("GATR_SOURCE_DIR").to_string());
    let src = Path::new(&src_dir);
    if !src.join("Cargo.toml").exists() {
        bail!(
            "source checkout not found at {src_dir} — this binary was likely installed from a \
             release archive; update with install.sh instead (or set GATR_SOURCE_DIR)"
        );
    }

    eprintln!("gatr upgrade: pulling {src_dir}");
    run(Command::new("git").args(["-C", &src_dir, "pull", "--ff-only"]))?;

    eprintln!("gatr upgrade: building release binary");
    run(Command::new("cargo").args([
        "build",
        "--release",
        "--manifest-path",
        &src.join("Cargo.toml").to_string_lossy(),
    ]))?;

    let new_bin = src.join("target").join("release").join(bin_name());
    let dest =
        std::fs::canonicalize(std::env::current_exe().context("cannot locate current executable")?)
            .context("cannot resolve current executable path")?;
    // Unlink first so the copy succeeds while this binary is still running.
    std::fs::remove_file(&dest)
        .with_context(|| format!("cannot replace {} (try sudo or reinstall)", dest.display()))?;
    std::fs::copy(&new_bin, &dest)
        .with_context(|| format!("failed to copy {} to {}", new_bin.display(), dest.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
    }

    let version = Command::new(&dest).arg("--version").output().map_or_else(
        |_| "unknown".to_string(),
        |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
    );
    eprintln!("gatr upgrade: installed {version} at {}", dest.display());
    Ok(0)
}

fn bin_name() -> &'static str {
    if cfg!(windows) {
        "gatr.exe"
    } else {
        "gatr"
    }
}

fn run(cmd: &mut Command) -> Result<()> {
    let program = cmd.get_program().to_string_lossy().to_string();
    let status = cmd
        .status()
        .with_context(|| format!("failed to run {program}"))?;
    if !status.success() {
        bail!("{program} exited with {status}");
    }
    Ok(())
}
