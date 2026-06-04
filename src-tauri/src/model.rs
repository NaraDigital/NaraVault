use serde::{Deserialize, Serialize};

/// A vault item. The whole struct is serialized to JSON and encrypted as a single
/// opaque blob — `name`, `sub` and `data` are all secret at rest. Only the random
/// `id` is stored in plaintext (it is the primary key and carries no information).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: String, // "login" | "card" | "seed" | "note"
    pub name: String,
    pub sub: String,
    #[serde(default)]
    pub fav: bool,
    /// Type-specific payload (username/password/words/...). Kept as free-form JSON
    /// to mirror the design and keep the schema flexible.
    pub data: serde_json::Value,
}

/// Non-secret projection of an item: everything needed to render a list row,
/// but never the `data` payload. Returned to the quick launcher window so the
/// decrypted secrets never leave the main webview's memory.
#[derive(Clone, Debug, Serialize)]
pub struct ItemMeta {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub name: String,
    pub sub: String,
    #[serde(default)]
    pub fav: bool,
}

/// Public vault status reported to the frontend (never includes secrets).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VaultPhase {
    Onboarding,
    Locked,
    Unlocked,
}

#[derive(Clone, Debug, Serialize)]
pub struct VaultStatus {
    pub phase: VaultPhase,
    pub hint: String,
}
