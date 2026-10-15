//! Build-time pack registration: ID lookup + contract-version compatibility
//! (06-language-packs.md §4). Owns no pack behavior. The production pack list
//! lives in the composition root (`apps/local`), keeping this crate free of
//! concrete-pack dependencies (02-workspace.md §3).

use language_pack_api::{CONTRACT_VERSION, LanguagePack, LanguagePackManifest};

/// Registration failure. Startup must surface this clearly and abort.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistryError {
    /// Pack compiled against a different plugin contract.
    #[error(
        "pack '{pack_id}' declares contract_version {pack_contract}, core requires {core_contract}"
    )]
    IncompatibleContract {
        /// Offending pack ID.
        pack_id: String,
        /// Contract version the pack declares.
        pack_contract: u32,
        /// [`CONTRACT_VERSION`] of this build.
        core_contract: u32,
    },
    /// Two packs registered the same ID.
    #[error("duplicate language pack id '{0}'")]
    DuplicateId(String),
}

/// Immutable set of registered packs, selectable by stable pack ID.
pub struct LanguagePackRegistry {
    packs: Vec<Box<dyn LanguagePack>>,
}

impl std::fmt::Debug for LanguagePackRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.packs.iter().map(|p| &p.manifest().id))
            .finish()
    }
}

impl LanguagePackRegistry {
    /// Validates and stores the given packs.
    ///
    /// # Errors
    ///
    /// [`RegistryError::IncompatibleContract`] when any pack's manifest
    /// `contract_version` differs from [`CONTRACT_VERSION`];
    /// [`RegistryError::DuplicateId`] when two packs share an ID.
    pub fn new(
        packs: impl IntoIterator<Item = Box<dyn LanguagePack>>,
    ) -> Result<Self, RegistryError> {
        let packs: Vec<Box<dyn LanguagePack>> = packs.into_iter().collect();
        let mut seen: Vec<&str> = Vec::with_capacity(packs.len());
        for pack in &packs {
            let manifest = pack.manifest();
            if manifest.contract_version != CONTRACT_VERSION {
                return Err(RegistryError::IncompatibleContract {
                    pack_id: manifest.id.clone(),
                    pack_contract: manifest.contract_version,
                    core_contract: CONTRACT_VERSION,
                });
            }
            if seen.contains(&manifest.id.as_str()) {
                return Err(RegistryError::DuplicateId(manifest.id.clone()));
            }
            seen.push(&manifest.id);
        }
        Ok(Self { packs })
    }

    /// Pack by stable ID (`learner_language_profiles.language_pack_id`).
    #[must_use]
    pub fn get(&self, pack_id: &str) -> Option<&dyn LanguagePack> {
        self.packs
            .iter()
            .find(|p| p.manifest().id == pack_id)
            .map(AsRef::as_ref)
    }

    /// Manifests of every registered pack, registration order.
    pub fn manifests(&self) -> impl Iterator<Item = &LanguagePackManifest> {
        self.packs.iter().map(|p| p.manifest())
    }
}
