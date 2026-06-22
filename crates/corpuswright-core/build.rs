use std::fs;
use std::path::Path;

fn resolved_dependency_version(lockfile: &str, package_name: &str) -> Option<String> {
    let mut in_target_package = false;
    for line in lockfile.lines() {
        let trimmed = line.trim();
        if trimmed == "[[package]]" {
            in_target_package = false;
            continue;
        }
        if let Some(name) = trimmed
            .strip_prefix("name = \"")
            .and_then(|value| value.strip_suffix('"'))
        {
            in_target_package = name == package_name;
            continue;
        }
        if in_target_package
            && let Some(version) = trimmed
                .strip_prefix("version = \"")
                .and_then(|value| value.strip_suffix('"'))
        {
            return Some(version.to_string());
        }
    }
    None
}

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR must be available to build scripts");
    let lockfile_path = Path::new(&manifest_dir).join("../../Cargo.lock");
    println!("cargo:rerun-if-changed={}", lockfile_path.display());

    let lockfile = fs::read_to_string(&lockfile_path).unwrap_or_else(|error| {
        panic!(
            "failed to read workspace lockfile at {}: {error}",
            lockfile_path.display()
        )
    });
    let version = resolved_dependency_version(&lockfile, "unicode-segmentation")
        .expect("unicode-segmentation must be present in Cargo.lock");
    println!("cargo:rustc-env=CORPUSWRIGHT_UNICODE_SEGMENTATION_VERSION={version}");
}
