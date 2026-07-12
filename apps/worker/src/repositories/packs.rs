//! `language_packs` metadata repository (WBS 4.2): registry manifests
//! persisted for audit + historical attribution (06-language-packs.md §4).

use serde::{Deserialize, Serialize};

/// Upsert pack metadata at startup/deploy.
pub const UPSERT_PACK: &str = "INSERT INTO language_packs
    (id, package_name, version, contract_version, target_language, target_locale,
     display_name, capabilities_json, enabled, installed_at, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, ?9)
 ON CONFLICT(id) DO UPDATE SET
    package_name = excluded.package_name,
    version = excluded.version,
    contract_version = excluded.contract_version,
    display_name = excluded.display_name,
    capabilities_json = excluded.capabilities_json,
    updated_at = excluded.updated_at";

/// All packs.
pub const SELECT_PACKS: &str = "SELECT id, package_name, version, contract_version,
    target_language, target_locale, display_name, capabilities_json, enabled
 FROM language_packs ORDER BY id";

/// One pack by ID.
pub const SELECT_PACK: &str = "SELECT id, package_name, version, contract_version,
    target_language, target_locale, display_name, capabilities_json, enabled
 FROM language_packs WHERE id = ?1";

/// `language_packs` row, column-shaped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackRow {
    /// `id`.
    pub id: String,
    /// `package_name`.
    pub package_name: String,
    /// `version`.
    pub version: String,
    /// `contract_version`.
    pub contract_version: u32,
    /// `target_language`.
    pub target_language: String,
    /// `target_locale`.
    pub target_locale: String,
    /// `display_name`.
    pub display_name: String,
    /// `capabilities_json` TEXT.
    pub capabilities_json: String,
    /// `enabled` 0/1.
    pub enabled: u8,
}

impl PackRow {
    /// Registry manifest → column row.
    ///
    /// # Errors
    ///
    /// Never fails in practice; `capabilities` serialization is infallible
    /// for the closed enum set.
    pub fn from_manifest(
        m: &language_pack_api::LanguagePackManifest,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            id: m.id.clone(),
            package_name: m.package.clone(),
            version: m.version.clone(),
            contract_version: m.contract_version,
            target_language: m.target_language.clone(),
            target_locale: m.target_locale.clone(),
            display_name: m.display_name.clone(),
            capabilities_json: serde_json::to_string(&m.capabilities)?,
            enabled: 1,
        })
    }
}
