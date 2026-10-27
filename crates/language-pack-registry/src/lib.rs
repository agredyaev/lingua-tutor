//! Build-time pack registration with dense index lookup and SoA manifest storage.

use language_pack_api::{CONTRACT_VERSION, Capability, LanguagePackManifest};
use serde::Serialize;

/// Dense registry index used only inside the process.
pub type PackIndex = u32;

/// Borrowed manifest view over the registry's SoA/CSR storage.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ManifestView<'a> {
    /// Stable external pack ID.
    pub id: &'a str,
    /// Crate package name.
    pub package: &'a str,
    /// Pack semantic version.
    pub version: &'a str,
    /// Core contract version.
    pub contract_version: u32,
    /// BCP 47 target language.
    pub target_language: &'a str,
    /// BCP 47 target locale.
    pub target_locale: &'a str,
    /// Human-readable pack name.
    pub display_name: &'a str,
    /// Supported instruction languages.
    pub supported_source_languages: &'a [String],
    /// Default instruction language.
    pub default_source_language: &'a str,
    /// Level framework identifier.
    pub level_framework: &'a str,
    /// Declared pack capabilities.
    pub capabilities: &'a [Capability],
}

/// Registration failure. Startup must surface this clearly and abort.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error(
        "pack '{pack_id}' declares contract_version {pack_contract}, core requires {core_contract}"
    )]
    /// A pack declares a contract version different from the core contract.
    IncompatibleContract {
        /// Stable external pack identifier.
        pack_id: String,
        /// Contract version declared by the pack.
        pack_contract: u32,
        /// Contract version required by the core.
        core_contract: u32,
    },
    /// Two registrations use the same stable external pack identifier.
    #[error("duplicate language pack id '{0}'")]
    DuplicateId(String),
}

/// Immutable parallel pack metadata columns plus aligned concrete pack owners.
///
/// Variable-length source-language and capability relations use flat value
/// buffers with prefix ends. External string IDs resolve once to dense indices.
pub struct LanguagePackRegistry<P> {
    manifest_id: Vec<String>,
    package: Vec<String>,
    version: Vec<String>,
    contract_version: Vec<u32>,
    target_language: Vec<String>,
    target_locale: Vec<String>,
    display_name: Vec<String>,
    default_source_language: Vec<String>,
    level_framework: Vec<String>,
    source_language_value: Vec<String>,
    source_language_end: Vec<u32>,
    capability_value: Vec<Capability>,
    capability_end: Vec<u32>,
    packs: Vec<P>,
}

impl<P> std::fmt::Debug for LanguagePackRegistry<P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_list()
            .entries(self.manifest_id.iter())
            .finish()
    }
}

/// Validate registrations and decompose manifest records into SoA/CSR storage.
pub fn build_registry<P>(
    registrations: impl IntoIterator<Item = (LanguagePackManifest, P)>,
) -> Result<LanguagePackRegistry<P>, RegistryError> {
    let mut registry = LanguagePackRegistry {
        manifest_id: Vec::new(),
        package: Vec::new(),
        version: Vec::new(),
        contract_version: Vec::new(),
        target_language: Vec::new(),
        target_locale: Vec::new(),
        display_name: Vec::new(),
        default_source_language: Vec::new(),
        level_framework: Vec::new(),
        source_language_value: Vec::new(),
        source_language_end: Vec::new(),
        capability_value: Vec::new(),
        capability_end: Vec::new(),
        packs: Vec::new(),
    };

    for (manifest, pack) in registrations {
        if manifest.contract_version != CONTRACT_VERSION {
            return Err(RegistryError::IncompatibleContract {
                pack_id: manifest.id,
                pack_contract: manifest.contract_version,
                core_contract: CONTRACT_VERSION,
            });
        }
        if registry.manifest_id.iter().any(|id| id == &manifest.id) {
            return Err(RegistryError::DuplicateId(manifest.id));
        }

        registry.manifest_id.push(manifest.id);
        registry.package.push(manifest.package);
        registry.version.push(manifest.version);
        registry.contract_version.push(manifest.contract_version);
        registry.target_language.push(manifest.target_language);
        registry.target_locale.push(manifest.target_locale);
        registry.display_name.push(manifest.display_name);
        registry
            .default_source_language
            .push(manifest.default_source_language);
        registry.level_framework.push(manifest.level_framework);
        registry
            .source_language_value
            .extend(manifest.supported_source_languages);
        registry
            .source_language_end
            .push(u32::try_from(registry.source_language_value.len()).unwrap_or(u32::MAX));
        registry.capability_value.extend(manifest.capabilities);
        registry
            .capability_end
            .push(u32::try_from(registry.capability_value.len()).unwrap_or(u32::MAX));
        registry.packs.push(pack);
    }

    Ok(registry)
}

/// Resolve an external stable pack ID to a dense internal index.
#[must_use]
pub fn index_of<P>(registry: &LanguagePackRegistry<P>, pack_id: &str) -> Option<PackIndex> {
    registry
        .manifest_id
        .iter()
        .position(|id| id == pack_id)
        .and_then(|index| u32::try_from(index).ok())
}

/// Pack data by external stable ID.
#[must_use]
pub fn get<'a, P>(registry: &'a LanguagePackRegistry<P>, pack_id: &str) -> Option<&'a P> {
    index_of(registry, pack_id).and_then(|index| pack_by_index(registry, index))
}

/// Pack data by dense internal index.
#[must_use]
pub fn pack_by_index<P>(registry: &LanguagePackRegistry<P>, index: PackIndex) -> Option<&P> {
    usize::try_from(index)
        .ok()
        .and_then(|index| registry.packs.get(index))
}

/// Manifest view by external stable ID.
#[must_use]
pub fn manifest<'a, P>(
    registry: &'a LanguagePackRegistry<P>,
    pack_id: &str,
) -> Option<ManifestView<'a>> {
    index_of(registry, pack_id).and_then(|index| manifest_by_index(registry, index))
}

/// Manifest view by dense internal index.
#[must_use]
pub fn manifest_by_index<P>(
    registry: &LanguagePackRegistry<P>,
    index: PackIndex,
) -> Option<ManifestView<'_>> {
    let index = usize::try_from(index).ok()?;
    let source_start = if index == 0 {
        0
    } else {
        usize::try_from(*registry.source_language_end.get(index - 1)?).ok()?
    };
    let source_end = usize::try_from(*registry.source_language_end.get(index)?).ok()?;
    let capability_start = if index == 0 {
        0
    } else {
        usize::try_from(*registry.capability_end.get(index - 1)?).ok()?
    };
    let capability_end = usize::try_from(*registry.capability_end.get(index)?).ok()?;

    Some(ManifestView {
        id: registry.manifest_id.get(index)?,
        package: registry.package.get(index)?,
        version: registry.version.get(index)?,
        contract_version: *registry.contract_version.get(index)?,
        target_language: registry.target_language.get(index)?,
        target_locale: registry.target_locale.get(index)?,
        display_name: registry.display_name.get(index)?,
        supported_source_languages: registry
            .source_language_value
            .get(source_start..source_end)?,
        default_source_language: registry.default_source_language.get(index)?,
        level_framework: registry.level_framework.get(index)?,
        capabilities: registry
            .capability_value
            .get(capability_start..capability_end)?,
    })
}

/// Manifest views in dense registration order.
pub fn manifests<P>(registry: &LanguagePackRegistry<P>) -> impl Iterator<Item = ManifestView<'_>> {
    (0..registry.manifest_id.len()).filter_map(|index| {
        u32::try_from(index)
            .ok()
            .and_then(|index| manifest_by_index(registry, index))
    })
}
