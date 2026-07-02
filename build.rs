use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/tags");
    println!("cargo:rerun-if-changed=src/version_shape.rs");

    // Baked-in path to the source checkout, used by `gatr upgrade` for
    // self-rebuild (overridable at runtime via the GATR_SOURCE_DIR env var).
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    println!("cargo:rustc-env=GATR_SOURCE_DIR={manifest_dir}");

    let fallback = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());
    let describe = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let version = if describe.is_empty() {
        fallback
    } else {
        shape_version(&describe, &fallback)
    };
    println!("cargo:rustc-env=GATR_VERSION={version}");
}

include!("src/version_shape.rs");
