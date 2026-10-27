//! `language_packs` metadata repository (WBS 4.2): registry manifests
//! persisted for audit + historical attribution (06-language-packs.md §4).

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
