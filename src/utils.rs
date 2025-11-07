use base64::prelude::*;
use chrono::{DateTime, Utc};
use prost::Message;
use crate::tamichat::protocol::*;

/// Encode a PublicKey as a base64url-encoded protobuf message
/// This matches the official implementation's format
pub fn encode_public_key_to_base64(public_key: &PublicKey) -> String {
    let mut bytes = Vec::new();
    public_key.encode(&mut bytes).expect("Failed to encode PublicKey");
    BASE64_URL_SAFE_NO_PAD.encode(&bytes)
}

/// Decode a base64url-encoded protobuf message into a PublicKey
#[allow(dead_code)]
pub fn decode_public_key_from_base64(encoded: &str) -> Result<PublicKey, String> {
    let bytes = BASE64_URL_SAFE_NO_PAD.decode(encoded.as_bytes())
        .map_err(|e| format!("Base64 decode error: {}", e))?;
    PublicKey::decode(&bytes[..])
        .map_err(|e| format!("Protobuf decode error: {}", e))
}

/// Returns the full base64 string (for backward compatibility or future use)
#[allow(dead_code)]
pub fn truncate_base64(encoded: String) -> String {
    encoded
}

/// Get the display version (truncated) of a base64 string for showing in UI
pub fn truncate_base64_display(encoded: &str, max_len: usize) -> String {
    if encoded.len() <= max_len {
        encoded.to_string()
    } else {
        format!("{}...", &encoded[..max_len])
    }
}

/// Format timestamp from milliseconds to HH:MM
pub fn format_timestamp(timestamp_ms: u64) -> String {
    let seconds = (timestamp_ms / 1000) as i64;
    let datetime = DateTime::from_timestamp(seconds, 0).unwrap_or_else(|| Utc::now());
    datetime.format("%H:%M").to_string()
}

/// Format a QueryReferencesRequest into a human-readable string
pub fn format_query_references_request(request: &QueryReferencesRequest) -> String {
    let mut output = String::new();
    
    // Format the reference
    if let Some(ref reference) = request.reference {
        output.push_str(&format!("Reference:\n"));
        output.push_str(&format!("  Type: {}\n", reference.reference_type));
        
        // Try to decode the reference based on type
        match reference.reference_type {
            2 => {
                // Pointer type
                if let Ok(pointer) = Pointer::decode(&reference.reference[..]) {
                    output.push_str("  Pointer:\n");
                    if let Some(ref system) = pointer.system {
                        output.push_str(&format!("    System: {}\n", encode_public_key_to_base64(system)));
                    }
                    if let Some(ref process) = pointer.process {
                        output.push_str(&format!("    Process: {}\n", BASE64_URL_SAFE_NO_PAD.encode(&process.process)));
                    }
                    output.push_str(&format!("    Logical Clock: {}\n", pointer.logical_clock));
                    if let Some(ref digest) = pointer.event_digest {
                        output.push_str(&format!("    Digest Type: {}\n", digest.digest_type));
                        output.push_str(&format!("    Digest: {}\n", BASE64_URL_SAFE_NO_PAD.encode(&digest.digest)));
                    }
                } else {
                    output.push_str(&format!("  Raw: {}\n", BASE64_URL_SAFE_NO_PAD.encode(&reference.reference)));
                }
            },
            3 => {
                // Byte reference (topic)
                if let Ok(topic) = String::from_utf8(reference.reference.clone()) {
                    output.push_str(&format!("  Topic: \"{}\"\n", topic));
                } else {
                    output.push_str(&format!("  Bytes: {}\n", BASE64_URL_SAFE_NO_PAD.encode(&reference.reference)));
                }
            },
            _ => {
                output.push_str(&format!("  Raw: {}\n", BASE64_URL_SAFE_NO_PAD.encode(&reference.reference)));
            }
        }
    }
    
    // Format cursor if present
    if let Some(ref cursor) = request.cursor {
        output.push_str(&format!("\nCursor: {}\n", BASE64_URL_SAFE_NO_PAD.encode(cursor)));
    }
    
    // Format request_events if present
    if let Some(ref req_events) = request.request_events {
        output.push_str("\nRequest Events:\n");
        if let Some(from_type) = req_events.from_type {
            output.push_str(&format!("  From Type: {}\n", from_type));
        }
        if !req_events.count_lww_element_references.is_empty() {
            output.push_str(&format!("  Count LWW Element References: {}\n", req_events.count_lww_element_references.len()));
        }
        if !req_events.count_references.is_empty() {
            output.push_str(&format!("  Count References: {}\n", req_events.count_references.len()));
        }
    }
    
    // Format count_lww_element_references
    if !request.count_lww_element_references.is_empty() {
        output.push_str(&format!("\nCount LWW Element References: {}\n", request.count_lww_element_references.len()));
        for (i, lww_ref) in request.count_lww_element_references.iter().enumerate() {
            output.push_str(&format!("  [{}]:\n", i));
            if let Ok(value_str) = String::from_utf8(lww_ref.value.clone()) {
                output.push_str(&format!("    Value: \"{}\"\n", value_str));
            } else {
                output.push_str(&format!("    Value: {}\n", BASE64_URL_SAFE_NO_PAD.encode(&lww_ref.value)));
            }
            if let Some(from_type) = lww_ref.from_type {
                output.push_str(&format!("    From Type: {}\n", from_type));
            }
        }
    }
    
    // Format count_references
    if !request.count_references.is_empty() {
        output.push_str(&format!("\nCount References: {}\n", request.count_references.len()));
        for (i, count_ref) in request.count_references.iter().enumerate() {
            output.push_str(&format!("  [{}]:\n", i));
            if let Some(from_type) = count_ref.from_type {
                output.push_str(&format!("    From Type: {}\n", from_type));
            }
        }
    }
    
    // Format extra_byte_references
    if !request.extra_byte_references.is_empty() {
        output.push_str(&format!("\nExtra Byte References: {}\n", request.extra_byte_references.len()));
        for (i, bytes) in request.extra_byte_references.iter().enumerate() {
            if let Ok(text) = String::from_utf8(bytes.clone()) {
                output.push_str(&format!("  [{}]: \"{}\"\n", i, text));
            } else {
                output.push_str(&format!("  [{}]: {}\n", i, BASE64_URL_SAFE_NO_PAD.encode(bytes)));
            }
        }
    }
    
    output
}
