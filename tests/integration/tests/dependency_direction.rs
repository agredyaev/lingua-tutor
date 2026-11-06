//! Architecture gate: generic core crates must not depend on removed
//! executable language-pack components.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

const REMOVED_RUNTIME_COMPONENTS: &[&str] = &[
    "language-pack-es-es",
    "synthetic-test-pack",
    "language-pack-registry",
];

fn core_manifests() -> Vec<(String, String)> {
    let crates_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates");
    fs::read_dir(crates_dir)
        .unwrap()
        .map(|e| e.unwrap().path().join("Cargo.toml"))
        .filter(|p| p.exists())
        .map(|p| (p.display().to_string(), fs::read_to_string(&p).unwrap()))
        .collect()
}

fn section_deps(manifest: &str, section: &str) -> Vec<String> {
    let mut in_section = false;
    let mut deps = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == format!("[{section}]");
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
fn no_core_crate_depends_on_removed_language_pack_components() {
    let manifests = core_manifests();
    assert!(!manifests.is_empty(), "crates/ scan found nothing");
    for (path, manifest) in &manifests {
        for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
            let dependencies = section_deps(manifest, section);
            for forbidden in REMOVED_RUNTIME_COMPONENTS {
                assert!(
                    !dependencies
                        .iter()
                        .any(|dependency| dependency == forbidden),
                    "{path}: {section} still depends on removed component {forbidden}"
                );
            }
        }
    }
}
