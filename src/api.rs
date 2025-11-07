use base64::prelude::*;
use prost::Message;
use ed25519_dalek::{SigningKey, Signature, Signer};
use rand::rngs::OsRng;
use rand::TryRngCore;
use chrono::Utc;
use crate::tamichat::protocol::*;

/// Generate a new identity (system keypair)
pub fn generate_identity() -> (SigningKey, PublicKey) {
    let mut rng = OsRng.unwrap_err();
    let signing_key = SigningKey::generate(&mut rng);
    let verifying_key = signing_key.verifying_key();
    
    let public_key = PublicKey {
        key_type: 1, // ed25519
        key: verifying_key.to_bytes().to_vec(),
    };
    
    (signing_key, public_key)
}

/// Generate a new random process ID
pub fn generate_process() -> Process {
    let mut process_bytes = [0u8; 16];
    rand::RngCore::fill_bytes(&mut OsRng.unwrap_err(), &mut process_bytes);
    
    Process {
        process: process_bytes.to_vec(),
    }
}

/// Create and sign a post event
pub fn create_post(
    signing_key: &SigningKey,
    public_key: &PublicKey,
    process: &Process,
    logical_clock: u64,
    content: String,
    topic: Option<String>,
) -> Result<SignedEvent, Box<dyn std::error::Error>> {
    // Create the Post content
    let post = Post {
        content: Some(content),
        images: vec![],
    };
    
    let mut post_bytes = Vec::new();
    post.encode(&mut post_bytes)?;
    
    // Get current unix milliseconds using chrono (wasm-compatible)
    let unix_milliseconds = Utc::now().timestamp_millis() as u64;
    
    // Create references for topic if provided
    let references = if let Some(topic_name) = topic {
        vec![Reference {
            reference_type: 3, // Byte reference type for topics
            reference: topic_name.as_bytes().to_vec(),
        }]
    } else {
        vec![]
    };
    
    // Create the Event
    let event = crate::tamichat::protocol::Event {
        system: Some(public_key.clone()),
        process: Some(process.clone()),
        logical_clock,
        content_type: 3, // Post
        content: post_bytes,
        vector_clock: Some(VectorClock {
            logical_clocks: vec![],
        }),
        indices: Some(Indices {
            indices: vec![],
        }),
        lww_element_set: None,
        lww_element: None,
        references,
        unix_milliseconds: Some(unix_milliseconds),
    };
    
    // Encode the event
    let mut event_bytes = Vec::new();
    event.encode(&mut event_bytes)?;
    
    // Sign the event
    let signature: Signature = signing_key.sign(&event_bytes);
    
    Ok(SignedEvent {
        signature: signature.to_bytes().to_vec(),
        event: event_bytes,
        moderation_tags: vec![],
    })
}

/// Create and sign a username event
pub fn create_username(
    signing_key: &SigningKey,
    public_key: &PublicKey,
    process: &Process,
    logical_clock: u64,
    username: String,
) -> Result<SignedEvent, Box<dyn std::error::Error>> {
    // Get current unix milliseconds using chrono (wasm-compatible)
    let unix_milliseconds = Utc::now().timestamp_millis() as u64;
    
    // Create the Event with LWW Element for username
    let event = crate::tamichat::protocol::Event {
        system: Some(public_key.clone()),
        process: Some(process.clone()),
        logical_clock,
        content_type: 5, // Username
        content: vec![],
        vector_clock: Some(VectorClock {
            logical_clocks: vec![],
        }),
        indices: Some(Indices {
            indices: vec![],
        }),
        lww_element_set: None,
        lww_element: Some(LwwElement {
            value: username.as_bytes().to_vec(),
            unix_milliseconds,
        }),
        references: vec![],
        unix_milliseconds: Some(unix_milliseconds),
    };
    
    // Encode the event
    let mut event_bytes = Vec::new();
    event.encode(&mut event_bytes)?;
    
    // Sign the event
    let signature: Signature = signing_key.sign(&event_bytes);
    
    Ok(SignedEvent {
        signature: signature.to_bytes().to_vec(),
        event: event_bytes,
        moderation_tags: vec![],
    })
}

/// Fetch data from the default API endpoint
pub async fn fetch_api_data() -> Result<QueryReferencesResponse, Box<dyn std::error::Error>> {
    let url = "https://serv1.polycentric.io/query_references?query=CmYIAhJiCiQIARIg1agZt9hNnBewSwAJ4b0HAzP5ujWZBLx43BE6nOYtuvgSEgoQSayWYqLjA0QDdZ3V_tkaWRgHIiQIARIgaWBgT3ALTZXnqqfRfLjAwJJUED_qYwofgA4X8nEhcHcaAggD&moderation_filters=[]";
    
    let response = reqwest::get(url).await?;
    let protobuf_bytes = response.bytes().await?;
    
    // Parse the protobuf directly from the raw bytes
    let query_response = QueryReferencesResponse::decode(&protobuf_bytes[..])?;
    
    Ok(query_response)
}

/// Fetch posts by topic
pub async fn fetch_topic_data(topic: &str) -> Result<QueryReferencesResponse, Box<dyn std::error::Error>> {
    // Build the QueryReferencesRequest
    let request = QueryReferencesRequest {
        reference: Some(Reference {
            reference_type: 3, // Byte reference type for topics
            reference: topic.as_bytes().to_vec(),
        }),
        cursor: None,
        request_events: Some(QueryReferencesRequestEvents {
            from_type: Some(3), // Content type 3 = Post
            count_lww_element_references: vec![],
            count_references: vec![],
        }),
        count_lww_element_references: vec![],
        count_references: vec![],
        extra_byte_references: vec![],
    };
    
    // Encode the request to protobuf
    let mut request_bytes = Vec::new();
    request.encode(&mut request_bytes)?;
    
    // Encode to base64 URL-safe
    let encoded_query = BASE64_URL_SAFE_NO_PAD.encode(&request_bytes);
    
    // Build the URL
    let url = format!(
        "https://serv1.polycentric.io/query_references?query={}&moderation_filters=[]",
        encoded_query
    );
    
    tracing::info!("Fetching topic '{}' with URL: {}", topic, url);
    
    let response = reqwest::get(&url).await?;
    let protobuf_bytes = response.bytes().await?;
    
    // Parse the protobuf response
    let query_response = QueryReferencesResponse::decode(&protobuf_bytes[..])?;
    
    Ok(query_response)
}

/// Fetch explore data
pub async fn fetch_explore_data() -> Result<ResultEventsAndRelatedEventsAndCursor, Box<dyn std::error::Error>> {
    let url = "https://serv1.polycentric.io/explore?limit=10";
    
    let response = reqwest::get(url).await?;
    let protobuf_bytes = response.bytes().await?;
    
    // Parse the protobuf directly from the raw bytes
    let explore_response = ResultEventsAndRelatedEventsAndCursor::decode(&protobuf_bytes[..])?;
    tracing::info!("Explore Response: {:?}", explore_response);
    
    Ok(explore_response)
}

/// Post events to the server
pub async fn post_events_to_server(signed_event: SignedEvent) -> Result<(), Box<dyn std::error::Error>> {
    let url = "https://serv1.polycentric.io/events";
    
    // Create Events message containing the signed event
    let events = Events {
        events: vec![signed_event],
    };
    
    // Encode to protobuf
    let mut protobuf_bytes = Vec::new();
    events.encode(&mut protobuf_bytes)?;
    
    // Post to server
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .header("Content-Type", "application/protobuf")
        .body(protobuf_bytes)
        .send()
        .await?;
    
    if response.status().is_success() {
        tracing::info!("Successfully posted event to server");
        Ok(())
    } else {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        Err(format!("Server returned error {}: {}", status, error_text).into())
    }
}
