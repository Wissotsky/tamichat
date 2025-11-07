use dioxus::prelude::*;
use prost::Message;
use base64::prelude::*;
use ed25519_dalek::{SigningKey, Signature, Signer};
use rand::rngs::OsRng;
use rand::TryRngCore;
use chrono::Utc;

pub mod tamichat {
    pub mod protocol {
        include!(concat!(env!("OUT_DIR"), "/tamichat.protocol.rs"));
    }
}

use tamichat::protocol::*;

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");
const HEADER_SVG: Asset = asset!("/assets/header.svg");

/// Safely truncate a base64-encoded string to a maximum length
fn truncate_base64(encoded: String, max_len: usize) -> String {
    if encoded.len() <= max_len {
        encoded
    } else {
        format!("{}...", &encoded[..max_len])
    }
}

/// Generate a new identity (system keypair)
fn generate_identity() -> (SigningKey, PublicKey) {
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
fn generate_process() -> Process {
    let mut process_bytes = [0u8; 16];
    rand::RngCore::fill_bytes(&mut OsRng.unwrap_err(), &mut process_bytes);
    
    Process {
        process: process_bytes.to_vec(),
    }
}

/// Create and sign a post event
fn create_post(
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
    let event = tamichat::protocol::Event {
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

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut current_page = use_signal(|| "query");
    
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        div {
            div {
                style: "margin-bottom: 20px;",
                button {
                    onclick: move |_| *current_page.write() = "query",
                    style: if current_page() == "query" { "font-weight: bold; margin-right: 10px;" } else { "margin-right: 10px;" },
                    "Query References"
                }
                button {
                    onclick: move |_| *current_page.write() = "explore",
                    style: if current_page() == "explore" { "font-weight: bold; margin-right: 10px;" } else { "margin-right: 10px;" },
                    "Explore"
                }
                button {
                    onclick: move |_| *current_page.write() = "create",
                    style: if current_page() == "create" { "font-weight: bold;" } else { "" },
                    "Create Post"
                }
            }
            
            match *current_page.read() {
                "query" => rsx! { DataFetcher {} },
                "explore" => rsx! { ExplorePage {} },
                "create" => rsx! { CreatePostPage {} },
                _ => rsx! { DataFetcher {} },
            }
        }
    }
}

#[component]
fn DataFetcher() -> Element {
    let mut data_state = use_signal(|| None::<QueryReferencesResponse>);
    let mut error_state = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);

    let fetch_data = move |_| {
        spawn(async move {
            *loading.write() = true;
            *error_state.write() = None;
            
            match fetch_api_data().await {
                Ok(response) => {
                    *data_state.write() = Some(response);
                }
                Err(e) => {
                    *error_state.write() = Some(format!("Error: {}", e));
                }
            }
            *loading.write() = false;
        });
    };

    rsx! {
        div {
            h1 { "Polycentric Query References" }
            
            button {
                onclick: fetch_data,
                disabled: loading(),
                if loading() { "Loading..." } else { "Fetch Data" }
            }

            if let Some(error) = error_state() {
                div {
                    "Error: {error}"
                }
            }

            if let Some(data) = data_state() {
                DataDisplay { data }
            }
        }
    }
}

#[component]
fn DataDisplay(data: QueryReferencesResponse) -> Element {
    rsx! {
        div {
            h2 { "Query References Response" }
            
            div {
                h3 { "Items ({data.items.len()})" }
                for (index, item) in data.items.iter().enumerate() {
                    EventItem { item: item.clone(), index }
                }
            }

            div {
                h3 { "Related Events ({data.related_events.len()})" }
                for (index, event) in data.related_events.iter().enumerate() {
                    RelatedEvent { event: event.clone(), index }
                }
            }

            if let Some(cursor) = &data.cursor {
                div {
                    h3 { "Cursor" }
                    pre {
                        {BASE64_STANDARD.encode(cursor)}
                    }
                }
            }

            div {
                h3 { "Counts ({data.counts.len()})" }
                for (index, count) in data.counts.iter().enumerate() {
                    div {
                        "Count {index}: {count}"
                    }
                }
            }
        }
    }
}

#[component]
fn EventItem(item: QueryReferencesResponseEventItem, index: usize) -> Element {
    rsx! {
        div {
            h4 { "Event Item {index + 1}" }
            
            if let Some(event) = &item.event {
                SignedEventDisplay { event: event.clone() }
            }

            div {
                strong { "Counts: " }
                for (i, count) in item.counts.iter().enumerate() {
                    span {
                        "{i}: {count}"
                    }
                }
            }
        }
    }
}

#[component]
fn RelatedEvent(event: SignedEvent, index: usize) -> Element {
    rsx! {
        div {
            h4 { "Related Event {index + 1}" }
            SignedEventDisplay { event }
        }
    }
}

#[component]
fn SignedEventDisplay(event: SignedEvent) -> Element {
    // Parse the Event from the raw bytes
    let parsed_event = tamichat::protocol::Event::decode(&event.event[..]).ok();
    
    rsx! {
        div {
            style: "border: 1px solid #ccc; margin: 10px; padding: 10px; border-radius: 5px;",
            
            div {
                strong { "Signature: " }
                span {
                    style: "font-family: monospace; font-size: 0.8em;",
                    {truncate_base64(BASE64_STANDARD.encode(&event.signature), 20)}
                }
            }

            if let Some(parsed) = parsed_event {
                EventDisplay { event: parsed }
            } else {
                div {
                    strong { "Raw Event Data: " }
                    span {
                        style: "font-family: monospace; font-size: 0.8em;",
                        {truncate_base64(BASE64_STANDARD.encode(&event.event), 50)}
                    }
                }
            }

            if !event.moderation_tags.is_empty() {
                div {
                    strong { "Moderation Tags: " }
                    for tag in &event.moderation_tags {
                        span {
                            style: "margin-right: 10px; background: #f0f0f0; padding: 2px 5px; border-radius: 3px;",
                            "{tag.name}: {tag.level}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EventDisplay(event: tamichat::protocol::Event) -> Element {
    let content_type_name = match event.content_type {
        1 => "Delete",
        2 => "SystemProcesses", 
        3 => "Post",
        4 => "Follow",
        5 => "Username",
        6 => "Description",
        7 => "BlobMeta",
        8 => "BlobSection",
        9 => "Avatar",
        10 => "Server",
        11 => "Vouch",
        12 => "Claim",
        13 => "Banner",
        _ => "Unknown",
    };

    rsx! {
        div {
            style: "background: #f9f9f9; padding: 8px; margin: 5px 0; border-radius: 3px;",
            
            div {
                strong { "System: " }
                span {
                    style: "font-family: monospace; font-size: 0.8em;",
                    {if let Some(ref system) = event.system {
                        truncate_base64(BASE64_STANDARD.encode(&system.key), 20)
                    } else {
                        "None".to_string()
                    }}
                }
            }

            div {
                strong { "Process: " }
                span {
                    style: "font-family: monospace; font-size: 0.8em;",
                    {if let Some(ref process) = event.process {
                        truncate_base64(BASE64_STANDARD.encode(&process.process), 16)
                    } else {
                        "None".to_string()
                    }}
                }
            }

            div {
                strong { "Logical Clock: " }
                span { "{event.logical_clock}" }
            }

            div {
                strong { "Content Type: " }
                span { "{event.content_type} ({content_type_name})" }
            }

            if let Some(unix_ms) = event.unix_milliseconds {
                div {
                    strong { "Timestamp: " }
                    span { "{unix_ms}" }
                }
            }

            // Parse content based on content_type
            ContentDisplay { content_type: event.content_type, content: event.content }

            // Display LWW Element if present
            if let Some(ref lww_element) = event.lww_element {
                div {
                    strong { "LWW Element: " }
                    span {
                        style: "font-family: monospace;",
                        {String::from_utf8_lossy(&lww_element.value).to_string()}
                    }
                    span { " (timestamp: {lww_element.unix_milliseconds})" }
                }
            }

            // Display LWW Element Set if present  
            if let Some(ref lww_set) = event.lww_element_set {
                div {
                    strong { "LWW Element Set: " }
                    span {
                        {match lww_set.operation {
                            0 => "ADD",
                            1 => "REMOVE", 
                            _ => "UNKNOWN"
                        }}
                    }
                    span {
                        style: "font-family: monospace; margin-left: 10px;",
                        {String::from_utf8_lossy(&lww_set.value).to_string()}
                    }
                    span { " (timestamp: {lww_set.unix_milliseconds})" }
                }
            }

            // Display references if present
            if !event.references.is_empty() {
                div {
                    strong { "References: " }
                    for (i, reference) in event.references.iter().enumerate() {
                        div {
                            style: "margin-left: 20px;",
                            "Reference {i + 1}: Type {reference.reference_type}"
                            br {}
                            span {
                                style: "font-family: monospace; font-size: 0.8em;",
                                {truncate_base64(BASE64_STANDARD.encode(&reference.reference), 30)}
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component] 
fn ContentDisplay(content_type: u64, content: Vec<u8>) -> Element {
    if content.is_empty() {
        return rsx! {
            div {
                style: "font-style: italic; color: #666;",
                "No content"
            }
        };
    }

    match content_type {
        3 => { // Post
            if let Ok(post) = Post::decode(&content[..]) {
                rsx! {
                    div {
                        strong { "Post Content: " }
                        if let Some(ref text) = post.content {
                            div {
                                style: "padding: 10px; border-radius: 5px; margin: 5px 0; border-left: 3px solid #007acc;",
                                {text.clone()}
                            }
                        }
                        if !post.images.is_empty() {
                            div {
                                strong { "Images: " }
                                for (i, image) in post.images.iter().enumerate() {
                                    div {
                                        style: "margin-left: 20px;",
                                        "Image {i + 1}: {image.mime} ({image.width}x{image.height}, {image.byte_count} bytes)"
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                rsx! {
                    div {
                        strong { "Post Content (raw): " }
                        span {
                            style: "font-family: monospace;",
                            {String::from_utf8_lossy(&content).to_string()}
                        }
                    }
                }
            }
        },
        12 => { // Claim
            if let Ok(claim) = Claim::decode(&content[..]) {
                rsx! {
                    div {
                        strong { "Claim: " }
                        div {
                            style: "margin-left: 20px;",
                            "Type: {claim.claim_type}"
                            for field in &claim.claim_fields {
                                div { "Field {field.key}: {field.value}" }
                            }
                            if !claim.images.is_empty() {
                                div {
                                    "Images: {claim.images.len()}"
                                }
                            }
                        }
                    }
                }
            } else {
                rsx! {
                    div {
                        strong { "Claim Content (raw): " }
                        span {
                            style: "font-family: monospace;",
                            {truncate_base64(BASE64_STANDARD.encode(&content), 50)}
                        }
                    }
                }
            }
        },
        1 => { // Delete
            if let Ok(delete) = Delete::decode(&content[..]) {
                let process_str = delete.process.as_ref()
                    .map(|p| truncate_base64(BASE64_STANDARD.encode(&p.process), 16))
                    .unwrap_or_else(|| "None".to_string());
                
                rsx! {
                    div {
                        strong { "Delete: " }
                        div {
                            style: "margin-left: 20px;",
                            "Process: {process_str}"
                            br {}
                            "Logical Clock: {delete.logical_clock}"
                            br {}
                            "Content Type: {delete.content_type}"
                        }
                    }
                }
            } else {
                rsx! {
                    div {
                        strong { "Delete Content (raw): " }
                        span {
                            style: "font-family: monospace;",
                            {truncate_base64(BASE64_STANDARD.encode(&content), 50)}
                        }
                    }
                }
            }
        },
        11 => { // Vouch
            rsx! {
                div {
                    strong { "Vouch" }
                    div {
                        style: "margin-left: 20px; font-style: italic;",
                        "This is a vouch event (empty content)"
                    }
                }
            }
        },
        _ => {
            // For other content types, try to display as text or show raw bytes
            if let Ok(text) = String::from_utf8(content.clone()) {
                rsx! {
                    div {
                        strong { "Content (text): " }
                        div {
                            style: "padding: 5px; border-radius: 3px; font-family: monospace;",
                            {text}
                        }
                    }
                }
            } else {
                rsx! {
                    div {
                        strong { "Content (raw bytes): " }
                        span {
                            style: "font-family: monospace; font-size: 0.8em;",
                            {truncate_base64(BASE64_STANDARD.encode(&content), 100)}
                        }
                    }
                }
            }
        }
    }
}

async fn fetch_api_data() -> Result<QueryReferencesResponse, Box<dyn std::error::Error>> {
    let url = "https://serv1.polycentric.io/query_references?query=CmYIAhJiCiQIARIg1agZt9hNnBewSwAJ4b0HAzP5ujWZBLx43BE6nOYtuvgSEgoQSayWYqLjA0QDdZ3V_tkaWRgHIiQIARIgaWBgT3ALTZXnqqfRfLjAwJJUED_qYwofgA4X8nEhcHcaAggD&moderation_filters=[]";
    
    let response = reqwest::get(url).await?;
    let protobuf_bytes = response.bytes().await?;
    
    // Parse the protobuf directly from the raw bytes
    let query_response = QueryReferencesResponse::decode(&protobuf_bytes[..])?;
    
    Ok(query_response)
}

#[component]
fn ExplorePage() -> Element {
    let mut data_state = use_signal(|| None::<ResultEventsAndRelatedEventsAndCursor>);
    let mut error_state = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);

    let fetch_data = move |_| {
        spawn(async move {
            *loading.write() = true;
            *error_state.write() = None;
            
            match fetch_explore_data().await {
                Ok(response) => {
                    *data_state.write() = Some(response);
                }
                Err(e) => {
                    *error_state.write() = Some(format!("Error: {}", e));
                }
            }
            *loading.write() = false;
        });
    };

    rsx! {
        div {
            h1 { "Explore" }
            
            button {
                onclick: fetch_data,
                disabled: loading(),
                if loading() { "Loading..." } else { "Fetch Explore Data" }
            }

            if let Some(error) = error_state() {
                div {
                    "Error: {error}"
                }
            }

            if let Some(data) = data_state() {
                ExploreDataDisplay { data }
            }
        }
    }
}

#[component]
fn ExploreDataDisplay(data: ResultEventsAndRelatedEventsAndCursor) -> Element {
    rsx! {
        div {
            if let Some(ref result_events) = data.result_events {
                div {
                    h2 { "Result Events ({result_events.events.len()})" }
                    
                    for (index, event) in result_events.events.iter().enumerate() {
                        div {
                            h4 { "Event {index + 1}" }
                            SignedEventDisplay { event: event.clone() }
                        }
                    }
                }
            }

            if let Some(ref related_events) = data.related_events {
                div {
                    h2 { "Related Events ({related_events.events.len()})" }
                    
                    for (index, event) in related_events.events.iter().enumerate() {
                        div {
                            h4 { "Related Event {index + 1}" }
                            SignedEventDisplay { event: event.clone() }
                        }
                    }
                }
            }

            if let Some(ref cursor) = data.cursor {
                div {
                    h3 { "Cursor" }
                    pre {
                        {BASE64_STANDARD.encode(cursor)}
                    }
                }
            }
        }
    }
}

// explore page
async fn fetch_explore_data() -> Result<ResultEventsAndRelatedEventsAndCursor, Box<dyn std::error::Error>> {
    let url = "https://serv1.polycentric.io/explore?limit=10";
    
    let response = reqwest::get(url).await?;
    let protobuf_bytes = response.bytes().await?;
    
    // Parse the protobuf directly from the raw bytes
    let explore_response = ResultEventsAndRelatedEventsAndCursor::decode(&protobuf_bytes[..])?;
    //print debug the response
    tracing::info!("Explore Response: {:?}", explore_response);
    
    Ok(explore_response)
}

// Post events to the server
async fn post_events_to_server(signed_event: SignedEvent) -> Result<(), Box<dyn std::error::Error>> {
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

#[component]
fn CreatePostPage() -> Element {
    let mut identity = use_signal(|| None::<(SigningKey, PublicKey, Process)>);
    let mut post_content = use_signal(|| String::new());
    let mut topic = use_signal(|| String::new());
    let mut logical_clock = use_signal(|| 1u64);
    let mut status_message = use_signal(|| None::<String>);
    let mut created_post = use_signal(|| None::<SignedEvent>);
    let mut is_posting = use_signal(|| false);
    
    let create_identity = move |_| {
        let (signing_key, public_key) = generate_identity();
        let process = generate_process();
        *identity.write() = Some((signing_key, public_key, process));
        *status_message.write() = Some("Identity created successfully!".to_string());
        *logical_clock.write() = 1;
    };
    
    let submit_post = move |_| {
        if let Some((ref signing_key, ref public_key, ref process)) = identity() {
            let content = post_content();
            
            if content.is_empty() {
                *status_message.write() = Some("Error: Post content cannot be empty".to_string());
                return;
            }
            
            // Clone values for async block
            let signing_key_clone = signing_key.clone();
            let public_key_clone = public_key.clone();
            let process_clone = process.clone();
            let current_clock = logical_clock();
            let topic_value = topic();
            let topic_opt = if topic_value.is_empty() { None } else { Some(topic_value) };
            
            spawn(async move {
                *is_posting.write() = true;
                *status_message.write() = Some("Creating and posting event...".to_string());
                
                match create_post(&signing_key_clone, &public_key_clone, &process_clone, current_clock, content, topic_opt) {
                    Ok(signed_event) => {
                        // Try to post to server
                        match post_events_to_server(signed_event.clone()).await {
                            Ok(()) => {
                                *created_post.write() = Some(signed_event.clone());
                                *status_message.write() = Some(format!("Post created and published successfully! Logical clock: {}", current_clock));
                                *logical_clock.write() = current_clock + 1;
                                *post_content.write() = String::new();
                                *topic.write() = String::new();
                            }
                            Err(e) => {
                                *status_message.write() = Some(format!("Error posting to server: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        *status_message.write() = Some(format!("Error creating post: {}", e));
                    }
                }
                
                *is_posting.write() = false;
            });
        } else {
            *status_message.write() = Some("Error: Please create an identity first".to_string());
        }
    };
    
    rsx! {
        div {
            style: "max-width: 800px; margin: 0 auto;",
            h1 { "Create Post" }
            
            // Identity Section
            div {
                style: "margin-bottom: 30px; padding: 20px; border: 1px solid #ddd; border-radius: 8px;",
                h2 { "Identity" }
                
                if let Some((_, ref public_key, ref process)) = identity() {
                    div {
                        div {
                            style: "margin-bottom: 10px;",
                            strong { "System (Public Key): " }
                            br {}
                            span {
                                style: "font-family: monospace; font-size: 0.9em; word-break: break-all;",
                                {BASE64_STANDARD.encode(&public_key.key)}
                            }
                        }
                        div {
                            style: "margin-bottom: 10px;",
                            strong { "Process ID: " }
                            br {}
                            span {
                                style: "font-family: monospace; font-size: 0.9em;",
                                {BASE64_STANDARD.encode(&process.process)}
                            }
                        }
                        div {
                            strong { "Logical Clock: " }
                            span { "{logical_clock()}" }
                        }
                    }
                } else {
                    div {
                        p { "No identity created yet. Create one to start posting." }
                        button {
                            onclick: create_identity,
                            style: "padding: 10px 20px; font-size: 16px; cursor: pointer;",
                            "Generate Identity"
                        }
                    }
                }
            }
            
            // Post Creation Section
            if identity().is_some() {
                div {
                    style: "margin-bottom: 30px; padding: 20px; border: 1px solid #ddd; border-radius: 8px;",
                    h2 { "Create a Post" }
                    
                    div {
                        style: "margin-bottom: 10px;",
                        label {
                            style: "display: block; margin-bottom: 5px; font-weight: bold;",
                            "Topic (optional)"
                        }
                        input {
                            r#type: "text",
                            value: "{topic}",
                            oninput: move |evt| *topic.write() = evt.value(),
                            placeholder: "Enter a topic name (e.g., technology, music)",
                            style: "width: 100%; padding: 8px; font-size: 14px; border: 1px solid #ccc; border-radius: 4px;",
                        }
                        small {
                            style: "color: #666; font-size: 12px;",
                            "Posts with topics can be discovered by others interested in the same topic"
                        }
                    }
                    
                    div {
                        style: "margin-bottom: 10px;",
                        label {
                            style: "display: block; margin-bottom: 5px; font-weight: bold;",
                            "Post Content"
                        }
                        textarea {
                            value: "{post_content}",
                            oninput: move |evt| *post_content.write() = evt.value(),
                            placeholder: "What's on your mind?",
                            style: "width: 100%; min-height: 150px; padding: 10px; font-size: 14px; border: 1px solid #ccc; border-radius: 4px; font-family: inherit;",
                        }
                    }
                    
                    button {
                        onclick: submit_post,
                        disabled: post_content().is_empty() || is_posting(),
                        style: "padding: 10px 20px; font-size: 16px; cursor: pointer; background-color: #007acc; color: white; border: none; border-radius: 4px;",
                        if is_posting() { "Posting..." } else { "Submit Post" }
                    }
                }
            }
            
            // Status Messages
            if let Some(ref message) = status_message() {
                div {
                    style: if message.starts_with("Error") {
                        "padding: 15px; margin-bottom: 20px; background-color: #fee; border: 1px solid #fcc; border-radius: 4px; color: #c00;"
                    } else {
                        "padding: 15px; margin-bottom: 20px; background-color: #efe; border: 1px solid #cfc; border-radius: 4px; color: #060;"
                    },
                    {message.clone()}
                }
            }
            
            // Display Created Post
            if let Some(ref post) = created_post() {
                div {
                    style: "padding: 20px; border: 1px solid #ddd; border-radius: 8px;",
                    h2 { "Last Created Post" }
                    SignedEventDisplay { event: post.clone() }
                }
            }
        }
    }
}
