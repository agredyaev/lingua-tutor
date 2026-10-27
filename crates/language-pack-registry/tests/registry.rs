//! Registry contract: dense lookup, contract-version gate, duplicate rejection.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use language_pack_api::LanguagePackManifest;
use language_pack_registry::{
    RegistryError, build_registry, get, index_of, manifest, manifests, pack_by_index,
};

fn registration(id: &str, contract_version: u32) -> (LanguagePackManifest, ()) {
    (
        LanguagePackManifest {
            id: id.to_owned(),
            package: format!("language-pack-{id}"),
            version: "0.1.0".to_owned(),
            contract_version,
            target_language: "zz".to_owned(),
            target_locale: "zz-ZZ".to_owned(),
            display_name: "Fake".to_owned(),
            supported_source_languages: vec!["ru".to_owned()],
            default_source_language: "ru".to_owned(),
            level_framework: "CEFR".to_owned(),
            capabilities: vec![],
        },
        (),
    )
}

#[test]
fn lookup_by_id_resolves_dense_index_and_pack() {
    let registry = build_registry([registration("aa-AA", 1), registration("bb-BB", 1)]).unwrap();

    assert_eq!(index_of(&registry, "aa-AA"), Some(0));
    assert_eq!(index_of(&registry, "bb-BB"), Some(1));
    assert!(pack_by_index(&registry, 1).is_some());
    assert_eq!(manifest(&registry, "bb-BB").unwrap().id, "bb-BB");
    assert!(get(&registry, "cc-CC").is_none());
}

#[test]
fn incompatible_contract_version_fails_with_pack_named() {
    let err = build_registry([registration("aa-AA", 999)]).unwrap_err();
    assert_eq!(
        err,
        RegistryError::IncompatibleContract {
            pack_id: "aa-AA".to_owned(),
            pack_contract: 999,
            core_contract: 1,
        }
    );
    assert!(
        err.to_string().contains("aa-AA"),
        "startup error must name the pack"
    );
}

#[test]
fn duplicate_pack_id_rejected() {
    let err = build_registry([registration("aa-AA", 1), registration("aa-AA", 1)]).unwrap_err();
    assert_eq!(err, RegistryError::DuplicateId("aa-AA".to_owned()));
}

#[test]
fn manifests_preserve_dense_registration_order() {
    let registry = build_registry([registration("aa-AA", 1), registration("bb-BB", 1)]).unwrap();
    let ids: Vec<_> = manifests(&registry).map(|manifest| manifest.id).collect();
    assert_eq!(ids, ["aa-AA", "bb-BB"]);
}
