//! Embed build provenance: `git describe` and the short commit SHA.
//!
//! Resolution order per value: explicit env var (how the Docker build passes
//! it in, since the image build context excludes `.git`) → local git → a
//! fallback marker. Never fails the build.

use std::process::Command;

fn main() {
    emit("DUCK_BUILD_VERSION", &["describe", "--tags", "--always", "--dirty"]);
    emit("DUCK_GIT_SHA", &["rev-parse", "--short=12", "HEAD"]);
    // Recompute when the checked-out commit moves.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-env-changed=DUCK_BUILD_VERSION");
    println!("cargo:rerun-if-env-changed=DUCK_GIT_SHA");
}

fn emit(var: &str, git_args: &[&str]) {
    let value = std::env::var(var)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            Command::new("git")
                .args(git_args)
                .output()
                .ok()
                .filter(|out| out.status.success())
                .and_then(|out| String::from_utf8(out.stdout).ok())
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env={var}={value}");
}
