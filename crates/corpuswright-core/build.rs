use std::fs;
use std::path::{Path, PathBuf};

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

    build_udpipe_pretokenised_bridge(Path::new(&manifest_dir), &lockfile);
}

fn build_udpipe_pretokenised_bridge(manifest_dir: &Path, lockfile: &str) {
    let native_dir = manifest_dir.join("native");
    let bridge_path = native_dir.join("udpipe_pretokenized.cpp");
    let udpipe_header_dir = locate_udpipe_header_dir(lockfile);

    println!("cargo:rerun-if-changed={}", bridge_path.display());

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .flag_if_supported("-std=c++11")
        .include(&native_dir)
        .include(&udpipe_header_dir)
        .file(&bridge_path);
    build.compile("corpuswright_udpipe_pretokenized");
}

fn locate_udpipe_header_dir(lockfile: &str) -> PathBuf {
    let version = resolved_dependency_version(lockfile, "udpipe-rs")
        .expect("udpipe-rs must be present in Cargo.lock");
    let source_root = cargo_registry_source_root();
    let crate_dir_name = format!("udpipe-rs-{version}");

    let entries = fs::read_dir(&source_root).unwrap_or_else(|error| {
        panic!(
            "failed to read Cargo registry source root at {}: {error}",
            source_root.display()
        )
    });

    for entry in entries {
        let entry = entry.expect("failed to read Cargo registry source entry");
        let header_dir = entry
            .path()
            .join(&crate_dir_name)
            .join("vendor/udpipe/src_lib_only");
        if header_dir.join("udpipe.h").exists() {
            return header_dir;
        }
    }

    panic!(
        "failed to locate udpipe-rs {version} public header under {}",
        source_root.display()
    );
}

fn cargo_registry_source_root() -> PathBuf {
    if let Ok(cargo_home) = std::env::var("CARGO_HOME") {
        return Path::new(&cargo_home).join("registry/src");
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        return Path::new(&home).join(".cargo/registry/src");
    }
    if let Ok(home) = std::env::var("HOME") {
        return Path::new(&home).join(".cargo/registry/src");
    }
    panic!("CARGO_HOME, USERPROFILE, or HOME must be set to locate udpipe-rs headers");
}
