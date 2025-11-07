use base64::prelude::*;
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

use crate::tamichat::protocol::{PublicKey, Process};

/// Serializable identity structure for storage
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StoredIdentity {
    /// Base64-encoded signing key (32 bytes)
    pub signing_key: String,
    /// Public key info
    pub public_key: StoredPublicKey,
    /// Process ID
    pub process: StoredProcess,
    /// Current logical clock value
    pub logical_clock: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StoredPublicKey {
    pub key_type: u64,
    pub key: String, // Base64-encoded
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StoredProcess {
    pub process: String, // Base64-encoded
}

impl StoredIdentity {
    /// Create from runtime identity components
    pub fn from_identity(
        signing_key: &SigningKey,
        public_key: &PublicKey,
        process: &Process,
        logical_clock: u64,
    ) -> Self {
        Self {
            signing_key: BASE64_STANDARD.encode(signing_key.to_bytes()),
            public_key: StoredPublicKey {
                key_type: public_key.key_type,
                key: BASE64_STANDARD.encode(&public_key.key),
            },
            process: StoredProcess {
                process: BASE64_STANDARD.encode(&process.process),
            },
            logical_clock,
        }
    }

    /// Convert to runtime identity components
    pub fn to_identity(&self) -> Result<(SigningKey, PublicKey, Process, u64), String> {
        // Decode signing key
        let signing_key_bytes = BASE64_STANDARD
            .decode(&self.signing_key)
            .map_err(|e| format!("Failed to decode signing key: {}", e))?;
        
        if signing_key_bytes.len() != 32 {
            return Err(format!(
                "Invalid signing key length: expected 32 bytes, got {}",
                signing_key_bytes.len()
            ));
        }

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&signing_key_bytes);
        let signing_key = SigningKey::from_bytes(&key_array);

        // Decode public key
        let public_key_bytes = BASE64_STANDARD
            .decode(&self.public_key.key)
            .map_err(|e| format!("Failed to decode public key: {}", e))?;

        let public_key = PublicKey {
            key_type: self.public_key.key_type,
            key: public_key_bytes,
        };

        // Decode process
        let process_bytes = BASE64_STANDARD
            .decode(&self.process.process)
            .map_err(|e| format!("Failed to decode process: {}", e))?;

        let process = Process {
            process: process_bytes,
        };

        Ok((signing_key, public_key, process, self.logical_clock))
    }
}

#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "tamichat_identity";

/// Save identity to localStorage
#[cfg(target_arch = "wasm32")]
pub fn save_identity(
    signing_key: &SigningKey,
    public_key: &PublicKey,
    process: &Process,
    logical_clock: u64,
) -> Result<(), String> {
    use web_sys::window;

    let stored = StoredIdentity::from_identity(signing_key, public_key, process, logical_clock);
    let json = serde_json::to_string(&stored)
        .map_err(|e| format!("Failed to serialize identity: {}", e))?;

    let window = window().ok_or("No window object available")?;
    let storage = window
        .local_storage()
        .map_err(|e| format!("Failed to access localStorage: {:?}", e))?
        .ok_or("localStorage not available")?;

    storage
        .set_item(STORAGE_KEY, &json)
        .map_err(|e| format!("Failed to save to localStorage: {:?}", e))?;

    tracing::info!("Identity saved to localStorage");
    Ok(())
}

/// Load identity from localStorage
#[cfg(target_arch = "wasm32")]
pub fn load_identity() -> Result<Option<(SigningKey, PublicKey, Process, u64)>, String> {
    use web_sys::window;

    let window = window().ok_or("No window object available")?;
    let storage = window
        .local_storage()
        .map_err(|e| format!("Failed to access localStorage: {:?}", e))?
        .ok_or("localStorage not available")?;

    let json = storage
        .get_item(STORAGE_KEY)
        .map_err(|e| format!("Failed to read from localStorage: {:?}", e))?;

    match json {
        Some(json_str) => {
            let stored: StoredIdentity = serde_json::from_str(&json_str)
                .map_err(|e| format!("Failed to deserialize identity: {}", e))?;
            
            let identity = stored.to_identity()?;
            tracing::info!("Identity loaded from localStorage");
            Ok(Some(identity))
        }
        None => {
            tracing::info!("No identity found in localStorage");
            Ok(None)
        }
    }
}

/// Delete identity from localStorage
#[cfg(target_arch = "wasm32")]
pub fn delete_identity() -> Result<(), String> {
    use web_sys::window;

    let window = window().ok_or("No window object available")?;
    let storage = window
        .local_storage()
        .map_err(|e| format!("Failed to access localStorage: {:?}", e))?
        .ok_or("localStorage not available")?;

    storage
        .remove_item(STORAGE_KEY)
        .map_err(|e| format!("Failed to delete from localStorage: {:?}", e))?;

    tracing::info!("Identity deleted from localStorage");
    Ok(())
}

/// Export identity as JSON string
pub fn export_identity_json(
    signing_key: &SigningKey,
    public_key: &PublicKey,
    process: &Process,
    logical_clock: u64,
) -> Result<String, String> {
    let stored = StoredIdentity::from_identity(signing_key, public_key, process, logical_clock);
    serde_json::to_string_pretty(&stored)
        .map_err(|e| format!("Failed to serialize identity: {}", e))
}

/// Import identity from JSON string
pub fn import_identity_json(json: &str) -> Result<(SigningKey, PublicKey, Process, u64), String> {
    let stored: StoredIdentity = serde_json::from_str(json)
        .map_err(|e| format!("Failed to deserialize identity: {}", e))?;
    stored.to_identity()
}

// Non-WASM stub implementations for compilation
#[cfg(not(target_arch = "wasm32"))]
pub fn save_identity(
    _signing_key: &SigningKey,
    _public_key: &PublicKey,
    _process: &Process,
    _logical_clock: u64,
) -> Result<(), String> {
    Err("Storage not available on non-WASM platforms".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_identity() -> Result<Option<(SigningKey, PublicKey, Process, u64)>, String> {
    Ok(None)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn delete_identity() -> Result<(), String> {
    Err("Storage not available on non-WASM platforms".to_string())
}
