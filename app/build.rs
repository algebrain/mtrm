use std::env;
use std::process::Command;
use time::{OffsetDateTime, macros::format_description};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs");

    let git_tag = latest_git_tag().unwrap_or_else(|| format!("v{}", cargo_pkg_version()));
    let build_timestamp = build_timestamp();

    println!("cargo:rustc-env=MTRM_GIT_TAG={git_tag}");
    println!("cargo:rustc-env=MTRM_BUILD_TIMESTAMP={build_timestamp}");
}

fn latest_git_tag() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let tag = String::from_utf8(output.stdout).ok()?;
    let tag = tag.trim();
    if tag.is_empty() {
        None
    } else {
        Some(tag.to_owned())
    }
}

fn cargo_pkg_version() -> String {
    env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_owned())
}

fn build_timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(format_description!(
            "[year repr:last_two][month][day]-[hour][minute][second]"
        ))
        .expect("static timestamp format must be valid")
}
