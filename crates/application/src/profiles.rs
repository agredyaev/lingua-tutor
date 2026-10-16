//! Language-profile creation rules (08-local-rpc.md §2).

use language_pack_api::{CONTRACT_VERSION, LanguagePackManifest};

/// Why a profile cannot be created against the given pack.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProfileCreationError {
    /// Pack contract incompatible with this build (`409 pack_incompatible`).
    #[error("pack '{0}' is contract-incompatible with this build")]
    PackIncompatible(String),
    /// Requested source language not supported by the pack.
    #[error("pack '{pack}' does not support source language '{source_language}'")]
    UnsupportedSourceLanguage {
        /// Pack ID.
        pack: String,
        /// Requested source language.
        source_language: String,
    },
}

/// Validates a profile-creation request against the target pack manifest.
///
/// # Errors
///
/// [`ProfileCreationError`] when the pack is incompatible or the source
/// language unsupported. One-active-profile-per-(learner, pack) is enforced
/// by `ux_llp_active_pack` at the database (04-database.md).
pub fn validate_profile_creation(
    manifest: &LanguagePackManifest,
    source_language: &str,
) -> Result<(), ProfileCreationError> {
    if manifest.contract_version != CONTRACT_VERSION {
        return Err(ProfileCreationError::PackIncompatible(manifest.id.clone()));
    }
    if !manifest
        .supported_source_languages
        .iter()
        .any(|s| s == source_language)
    {
        return Err(ProfileCreationError::UnsupportedSourceLanguage {
            pack: manifest.id.clone(),
            source_language: source_language.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use language_pack_api::LanguagePackManifest;

    fn manifest(contract: u32) -> LanguagePackManifest {
        LanguagePackManifest {
            id: "es-ES".to_owned(),
            package: "language-pack-es-es".to_owned(),
            version: "0.1.0".to_owned(),
            contract_version: contract,
            target_language: "es".to_owned(),
            target_locale: "es-ES".to_owned(),
            display_name: "Spanish (Spain)".to_owned(),
            supported_source_languages: vec!["ru".to_owned(), "en".to_owned()],
            default_source_language: "ru".to_owned(),
            level_framework: "CEFR".to_owned(),
            capabilities: vec![],
        }
    }

    #[test]
    fn accepts_supported_source() {
        assert_eq!(validate_profile_creation(&manifest(1), "ru"), Ok(()));
    }

    #[test]
    fn rejects_unsupported_source_and_bad_contract() {
        assert!(matches!(
            validate_profile_creation(&manifest(1), "fr"),
            Err(ProfileCreationError::UnsupportedSourceLanguage { .. })
        ));
        assert!(matches!(
            validate_profile_creation(&manifest(2), "ru"),
            Err(ProfileCreationError::PackIncompatible(_))
        ));
    }
}
