//! Architecture gate: no generic core crate depends on a concrete language
//! pack (02-workspace.md §3, 12-testing.md §10). Scans `crates/*/Cargo.toml`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

const CONCRETE_PACK: &str = "language-pack-es-es";
const TEST_PACK: &str = "synthetic-test-pack";

fn core_manifests() -> Vec<(String, String)> {
    let crates_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../crates");
    fs::read_dir(crates_dir)
        .unwrap()
        .map(|e| e.unwrap().path().join("Cargo.toml"))
        .filter(|p| p.exists())
        .map(|p| (p.display().to_string(), fs::read_to_string(&p).unwrap()))
        .collect()
}

/// Dependency names listed in the given Cargo.toml section.
fn section_deps(manifest: &str, section: &str) -> Vec<String> {
    let mut in_section = false;
    let mut deps = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == format!("[{section}]");
            // `[dependencies.NAME]` table syntax declares NAME directly.
            if let Some(name) = line
                .strip_prefix(&format!("[{section}."))
                .and_then(|rest| rest.strip_suffix(']'))
            {
                deps.push(name.trim().to_owned());
            }
            continue;
        }
        if in_section
            && !line.is_empty()
            && !line.starts_with('#')
            && let Some((name, _)) = line.split_once('=')
        {
            deps.push(name.trim().to_owned());
        }
    }
    deps
}

#[test]
fn no_core_crate_depends_on_concrete_pack() {
    let manifests = core_manifests();
    assert!(!manifests.is_empty(), "crates/ scan found nothing");
    for (path, manifest) in &manifests {
        let runtime = section_deps(manifest, "dependencies");
        assert!(
            !runtime.contains(&CONCRETE_PACK.to_owned()),
            "{path}: core crate depends on {CONCRETE_PACK}"
        );
        // Synthetic pack allowed only as dev-dependency (core CI), never runtime.
        assert!(
            !runtime.contains(&TEST_PACK.to_owned()),
            "{path}: core crate has runtime dependency on {TEST_PACK}"
        );
        let dev = section_deps(manifest, "dev-dependencies");
        assert!(
            !dev.contains(&CONCRETE_PACK.to_owned()),
            "{path}: core crate test-depends on {CONCRETE_PACK}"
        );
    }
}
