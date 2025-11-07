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
// const HEADER_SVG: Asset = asset!("/assets/header.svg");

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

/// Create and sign a username event
fn create_username(
    signing_key: &SigningKey,
    public_key: &PublicKey,
    process: &Process,
    logical_clock: u64,
    username: String,
) -> Result<SignedEvent, Box<dyn std::error::Error>> {
    // Get current unix milliseconds using chrono (wasm-compatible)
    let unix_milliseconds = Utc::now().timestamp_millis() as u64;
    
    // Create the Event with LWW Element for username
    let event = tamichat::protocol::Event {
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

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut current_page = use_signal(|| "chat");
    
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        
        if current_page() == "debug" {
            DebugPages { 
                on_back: move |_| *current_page.write() = "chat" 
            }
        } else {
            ChatPage {}
            a {
                class: "debug-link",
                onclick: move |_| *current_page.write() = "debug",
                "🔧 Debug"
            }
        }
    }
}

#[derive(Clone, Debug)]
struct ChatMessage {
    content: String,
    timestamp: u64,
    system_key: String,
}

#[component]
fn ChatPage() -> Element {
    let mut messages = use_signal(|| Vec::<ChatMessage>::new());
    let mut message_input = use_signal(|| String::new());
    let mut identity = use_signal(|| None::<(SigningKey, PublicKey, Process, u64)>);
    let mut is_sending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    
    // Helper function to scroll chat to bottom
    let scroll_to_bottom = move || {
        document::eval(
            r#"
            const messagesDiv = document.getElementById('messages');
            if (messagesDiv) {
                messagesDiv.scrollTop = messagesDiv.scrollHeight;
            }
            "#
        );
    };
    
    // Auto-create identity on mount
    use_effect(move || {
        if identity().is_none() {
            let (signing_key, public_key) = generate_identity();
            let process = generate_process();
            *identity.write() = Some((signing_key, public_key, process, 1));
        }
    });
    
    // Initial fetch
    use_effect(move || {
        spawn(async move {
            if let Err(e) = fetch_and_update_messages(&mut messages, &mut error).await {
                tracing::error!("Error fetching messages: {}", e);
            } else {
                // Scroll to bottom after initial load
                scroll_to_bottom();
            }
        });
    });
    
    // Auto-refresh messages every 5 seconds
    use_effect(move || {
        spawn(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(5_000).await;
                if let Err(e) = fetch_and_update_messages(&mut messages, &mut error).await {
                    tracing::error!("Error auto-fetching messages: {}", e);
                }
            }
        });
    });
    
    let send_message = move |_evt| {
        let content = message_input();
        if content.trim().is_empty() || is_sending() {
            return;
        }
        
        if let Some((ref signing_key, ref public_key, ref process, ref clock)) = identity() {
            let signing_key_clone = signing_key.clone();
            let public_key_clone = public_key.clone();
            let process_clone = process.clone();
            let current_clock = *clock;
            
            let mut is_sending = is_sending.clone();
            let mut error = error.clone();
            let mut message_input = message_input.clone();
            let mut messages = messages.clone();
            let mut identity = identity.clone();
            
            spawn(async move {
                *is_sending.write() = true;
                *error.write() = None;
                
                match create_post(
                    &signing_key_clone,
                    &public_key_clone,
                    &process_clone,
                    current_clock,
                    content.clone(),
                    Some("tamichat".to_string())
                ) {
                    Ok(signed_event) => {
                        match post_events_to_server(signed_event).await {
                            Ok(()) => {
                                // Update local identity clock
                                if let Some((sk, pk, proc, _)) = identity() {
                                    *identity.write() = Some((sk, pk, proc, current_clock + 1));
                                }
                                *message_input.write() = String::new();
                                
                                // Refresh messages immediately
                                let _ = fetch_and_update_messages(&mut messages, &mut error).await;
                                
                                // Scroll to bottom after sending
                                scroll_to_bottom();
                            }
                            Err(e) => {
                                *error.write() = Some(format!("Failed to send: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        *error.write() = Some(format!("Error creating message: {}", e));
                    }
                }
                
                *is_sending.write() = false;
            });
        }
    };
    
    rsx! {
        div {
            class: "chat-container",
            
            div {
                class: "chat-header",
                h1 { "TamiChat" }
                p { "Shoutbox bolted on top of polycentric" }
            }
            
            div {
                class: "chat-messages",
                id: "messages",
                
                if messages().is_empty() {
                    p {
                        class: "loading-indicator",
                        "Loading messages..."
                    }
                }
                
                for message in messages().iter() {
                    div {
                        class: "message",
                        p {
                            class: "message-content",
                            {message.content.clone()}
                        }
                        small {
                            class: "message-meta",
                            span {
                                class: "message-time",
                                {format_timestamp(message.timestamp)}
                            }
                            " - "
                            span {
                                class: "message-system",
                                {message.system_key[..8].to_string()}
                            }
                        }
                    }
                }
            }
            
            if let Some(ref err) = error() {
                p {
                    class: "error-message",
                    strong { "Error: " }
                    {err.clone()}
                }
            }
            
            if is_sending() {
                p {
                    class: "sending-indicator",
                    "Sending message..."
                }
            }
            
            div {
                class: "chat-input-container",
                label {
                    r#for: "message-input",
                    strong { "Post a message:" }
                }
                div {
                    class: "chat-input-wrapper",
                    input {
                        id: "message-input",
                        class: "chat-input",
                        r#type: "text",
                        placeholder: "Type your message...",
                        value: "{message_input}",
                        disabled: is_sending(),
                        oninput: move |evt| *message_input.write() = evt.value(),
                        onkeydown: move |evt| {
                            if evt.key() == Key::Enter && !is_sending() {
                                let content = message_input();
                                if content.trim().is_empty() {
                                    return;
                                }
                                
                                if let Some((ref signing_key, ref public_key, ref process, ref clock)) = identity() {
                                    let signing_key_clone = signing_key.clone();
                                    let public_key_clone = public_key.clone();
                                    let process_clone = process.clone();
                                    let current_clock = *clock;
                                    
                                    spawn(async move {
                                        *is_sending.write() = true;
                                        *error.write() = None;
                                        
                                        match create_post(
                                            &signing_key_clone,
                                            &public_key_clone,
                                            &process_clone,
                                            current_clock,
                                            content.clone(),
                                            Some("tamichat".to_string())
                                        ) {
                                            Ok(signed_event) => {
                                                match post_events_to_server(signed_event).await {
                                                    Ok(()) => {
                                                        if let Some((sk, pk, proc, _)) = identity() {
                                                            *identity.write() = Some((sk, pk, proc, current_clock + 1));
                                                        }
                                                        *message_input.write() = String::new();
                                                        let _ = fetch_and_update_messages(&mut messages, &mut error).await;
                                                        
                                                        // Scroll to bottom after sending
                                                        scroll_to_bottom();
                                                    }
                                                    Err(e) => {
                                                        *error.write() = Some(format!("Failed to send: {}", e));
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                *error.write() = Some(format!("Error creating message: {}", e));
                                            }
                                        }
                                        
                                        *is_sending.write() = false;
                                    });
                                }
                            }
                        },
                    }
                    br {}
                    button {
                        class: "chat-send-btn",
                        onclick: send_message,
                        disabled: message_input().trim().is_empty() || is_sending(),
                        if is_sending() { "Sending..." } else { "Send" }
                    }
                }
            }
        }
    }
}

async fn fetch_and_update_messages(
    messages: &mut Signal<Vec<ChatMessage>>,
    error: &mut Signal<Option<String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    match fetch_topic_data("tamichat").await {
        Ok(response) => {
            let mut new_messages = Vec::new();
            
            for item in response.items {
                if let Some(event) = item.event {
                    if let Ok(parsed_event) = tamichat::protocol::Event::decode(&event.event[..]) {
                        if parsed_event.content_type == 3 {
                            if let Ok(post) = Post::decode(&parsed_event.content[..]) {
                                if let Some(content) = post.content {
                                    let system_key = parsed_event.system
                                        .as_ref()
                                        .map(|s| BASE64_STANDARD.encode(&s.key))
                                        .unwrap_or_default();
                                    
                                    new_messages.push(ChatMessage {
                                        content,
                                        timestamp: parsed_event.unix_milliseconds.unwrap_or(0),
                                        system_key,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            
            // Sort by timestamp
            new_messages.sort_by_key(|m| m.timestamp);
            *messages.write() = new_messages;
            *error.write() = None;
            Ok(())
        }
        Err(e) => {
            *error.write() = Some(format!("Failed to load messages: {}", e));
            Err(e)
        }
    }
}

fn format_timestamp(timestamp_ms: u64) -> String {
    use chrono::{DateTime, Utc};
    let seconds = (timestamp_ms / 1000) as i64;
    let datetime = DateTime::from_timestamp(seconds, 0).unwrap_or_else(|| Utc::now());
    datetime.format("%H:%M").to_string()
}

#[component]
fn DebugPages(on_back: EventHandler<()>) -> Element {
    let mut current_page = use_signal(|| "query");
    
    rsx! {
        div {
            class: "debug-container",
            
            div {
                class: "debug-nav",
                button {
                    class: "debug-btn",
                    onclick: move |_| on_back.call(()),
                    "← Back to Chat"
                }
                " "
                button {
                    class: if current_page() == "query" { "debug-btn active" } else { "debug-btn" },
                    onclick: move |_| *current_page.write() = "query",
                    "Query References"
                }
                " "
                button {
                    class: if current_page() == "explore" { "debug-btn active" } else { "debug-btn" },
                    onclick: move |_| *current_page.write() = "explore",
                    "Explore"
                }
                " "
                button {
                    class: if current_page() == "create" { "debug-btn active" } else { "debug-btn" },
                    onclick: move |_| *current_page.write() = "create",
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
    let mut decoder_input = use_signal(|| String::new());
    let mut decoded_request = use_signal(|| None::<String>);
    let mut topic_query = use_signal(|| String::new());

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
    
    let fetch_topic = move |_| {
        let topic = topic_query();
        if topic.is_empty() {
            *error_state.write() = Some("Error: Topic cannot be empty".to_string());
            return;
        }
        
        spawn(async move {
            *loading.write() = true;
            *error_state.write() = None;
            
            match fetch_topic_data(&topic).await {
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
    
    let decode_request = move |_| {
        let input = decoder_input();
        if input.is_empty() {
            *decoded_request.write() = Some("Error: Input is empty".to_string());
            return;
        }
        
        match BASE64_URL_SAFE_NO_PAD.decode(input.as_bytes()) {
            Ok(bytes) => {
                match QueryReferencesRequest::decode(&bytes[..]) {
                    Ok(request) => {
                        *decoded_request.write() = Some(format_query_references_request(&request));
                    }
                    Err(e) => {
                        *decoded_request.write() = Some(format!("Error decoding protobuf: {}", e));
                    }
                }
            }
            Err(e) => {
                *decoded_request.write() = Some(format!("Error decoding base64: {}", e));
            }
        }
    };

    rsx! {
        div {
            class: "container",
            h1 { "Polycentric Query References" }
            
            hr {}
            
            // Topic Query Section
            div {
                class: "topic-query-section",
                h2 { "Query by Topic" }
                p {
                    class: "topic-query-description",
                    "Enter a topic name to fetch all posts tagged with that topic:"
                }
                
                label {
                    r#for: "topic-input",
                    strong { "Topic name:" }
                }
                br {}
                input {
                    id: "topic-input",
                    r#type: "text",
                    value: "{topic_query}",
                    oninput: move |evt| *topic_query.write() = evt.value(),
                    placeholder: "e.g., technology, music, tamichat",
                    class: "topic-query-input",
                }
                br {}
                button {
                    onclick: fetch_topic,
                    disabled: topic_query().is_empty() || loading(),
                    class: "topic-query-btn",
                    if loading() { "Loading..." } else { "Fetch Topic" }
                }
            }
            
            hr {}
            
            // Query Decoder Section
            div {
                class: "query-decoder-section",
                h2 { "Query Request Decoder" }
                p {
                    class: "topic-query-description",
                    "Paste a base64 URL-safe encoded QueryReferencesRequest to decode and inspect it:"
                }
                
                label {
                    r#for: "decoder-input",
                    strong { "Base64 encoded query:" }
                }
                br {}
                textarea {
                    id: "decoder-input",
                    value: "{decoder_input}",
                    oninput: move |evt| *decoder_input.write() = evt.value(),
                    placeholder: "Paste base64 URL-safe encoded query here (e.g., CmYIAhJiCiQIARIg1agZt9hNnBew...)",
                    class: "decoder-textarea",
                }
                
                button {
                    onclick: decode_request,
                    disabled: decoder_input().is_empty(),
                    class: "decode-btn",
                    "Decode Query"
                }
                
                if let Some(ref decoded) = decoded_request() {
                    div {
                        class: "decoded-output",
                        h3 { "Decoded Request:" }
                        pre {
                            class: "decoded-pre",
                            {decoded.clone()}
                        }
                    }
                }
            }
            
            hr {}
            
            button {
                onclick: fetch_data,
                disabled: loading(),
                if loading() { "Loading..." } else { "Fetch Default Data" }
            }

            if let Some(error) = error_state() {
                p {
                    class: "error-message",
                    strong { "Error: " }
                    {error}
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
            class: "box",
            h2 { "Query References Response" }
            
            h3 { "Items ({data.items.len()})" }
            for (index, item) in data.items.iter().enumerate() {
                EventItem { item: item.clone(), index }
            }

            h3 { "Related Events ({data.related_events.len()})" }
            for (index, event) in data.related_events.iter().enumerate() {
                RelatedEvent { event: event.clone(), index }
            }

            if let Some(cursor) = &data.cursor {
                h3 { "Cursor" }
                pre {
                    class: "mono",
                    {BASE64_STANDARD.encode(cursor)}
                }
            }

            h3 { "Counts ({data.counts.len()})" }
            for (index, count) in data.counts.iter().enumerate() {
                p {
                    "Count {index}: {count}"
                }
            }
        }
    }
}

#[component]
fn EventItem(item: QueryReferencesResponseEventItem, index: usize) -> Element {
    rsx! {
        div {
            class: "box",
            h4 { "Event Item {index + 1}" }
            
            if let Some(event) = &item.event {
                SignedEventDisplay { event: event.clone() }
            }

            p {
                strong { "Counts: " }
                for (i, count) in item.counts.iter().enumerate() {
                    "{i}: {count} "
                }
            }
        }
    }
}

#[component]
fn RelatedEvent(event: SignedEvent, index: usize) -> Element {
    rsx! {
        div {
            class: "box",
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
            class: "event-item-border",
            
            p {
                strong { "Signature: " }
                code {
                    class: "signature-span",
                    {truncate_base64(BASE64_STANDARD.encode(&event.signature), 20)}
                }
            }

            if let Some(parsed) = parsed_event {
                EventDisplay { event: parsed }
            } else {
                p {
                    strong { "Raw Event Data: " }
                    code {
                        class: "signature-span",
                        {truncate_base64(BASE64_STANDARD.encode(&event.event), 50)}
                    }
                }
            }

            if !event.moderation_tags.is_empty() {
                p {
                    strong { "Moderation Tags: " }
                    for tag in &event.moderation_tags {
                        span {
                            class: "moderation-tag",
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
            class: "event-detail-bg",
            
            p {
                strong { "System: " }
                code {
                    class: "system-key-span",
                    {if let Some(ref system) = event.system {
                        truncate_base64(BASE64_STANDARD.encode(&system.key), 20)
                    } else {
                        "None".to_string()
                    }}
                }
            }

            p {
                strong { "Process: " }
                code {
                    class: "process-key-span",
                    {if let Some(ref process) = event.process {
                        truncate_base64(BASE64_STANDARD.encode(&process.process), 16)
                    } else {
                        "None".to_string()
                    }}
                }
            }

            p {
                strong { "Logical Clock: " }
                {format!("{}", event.logical_clock)}
            }

            p {
                strong { "Content Type: " }
                {format!("{} ({})", event.content_type, content_type_name)}
            }

            if let Some(unix_ms) = event.unix_milliseconds {
                p {
                    strong { "Timestamp: " }
                    {format!("{}", unix_ms)}
                }
            }

            // Parse content based on content_type
            ContentDisplay { content_type: event.content_type, content: event.content }

            // Display LWW Element if present
            if let Some(ref lww_element) = event.lww_element {
                p {
                    strong { "LWW Element: " }
                    code {
                        class: "raw-content-span",
                        {String::from_utf8_lossy(&lww_element.value).to_string()}
                    }
                    " (timestamp: {lww_element.unix_milliseconds})"
                }
            }

            // Display LWW Element Set if present  
            if let Some(ref lww_set) = event.lww_element_set {
                p {
                    strong { "LWW Element Set: " }
                    {match lww_set.operation {
                        0 => "ADD",
                        1 => "REMOVE", 
                        _ => "UNKNOWN"
                    }}
                    " "
                    code {
                        class: "raw-content-span",
                        {String::from_utf8_lossy(&lww_set.value).to_string()}
                    }
                    " (timestamp: {lww_set.unix_milliseconds})"
                }
            }

            // Display references if present
            if !event.references.is_empty() {
                p {
                    strong { "References: " }
                }
                for (i, reference) in event.references.iter().enumerate() {
                    div {
                        class: "reference-details",
                        "Reference {i + 1}: Type {reference.reference_type}"
                        br {}
                        code {
                            class: "system-key-span",
                            {truncate_base64(BASE64_STANDARD.encode(&reference.reference), 30)}
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
            p {
                class: "no-content-message",
                em { "No content" }
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
                            blockquote {
                                class: "post-content-box",
                                {text.clone()}
                            }
                        }
                        if !post.images.is_empty() {
                            p {
                                strong { "Images: " }
                            }
                            for (i, image) in post.images.iter().enumerate() {
                                p {
                                    class: "image-item",
                                    "Image {i + 1}: {image.mime} ({image.width}x{image.height}, {image.byte_count} bytes)"
                                }
                            }
                        }
                    }
                }
            } else {
                rsx! {
                    p {
                        strong { "Post Content (raw): " }
                        code {
                            class: "raw-content-span",
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
                            class: "reference-details",
                            p { "Type: {claim.claim_type}" }
                            for field in &claim.claim_fields {
                                p { "Field {field.key}: {field.value}" }
                            }
                            if !claim.images.is_empty() {
                                p { "Images: {claim.images.len()}" }
                            }
                        }
                    }
                }
            } else {
                rsx! {
                    p {
                        strong { "Claim Content (raw): " }
                        code {
                            class: "raw-content-span",
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
                            class: "delete-details",
                            p {
                                "Process: "
                                code { {process_str} }
                            }
                            p { "Logical Clock: {delete.logical_clock}" }
                            p { "Content Type: {delete.content_type}" }
                        }
                    }
                }
            } else {
                rsx! {
                    p {
                        strong { "Delete Content (raw): " }
                        code {
                            class: "raw-content-span",
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
                    p {
                        class: "vouch-notice",
                        em { "This is a vouch event (empty content)" }
                    }
                }
            }
        },
        _ => {
            // For other content types, try to display as text or show raw bytes
            if let Ok(text) = String::from_utf8(content.clone()) {
                rsx! {
                    p {
                        strong { "Content (text): " }
                        pre {
                            class: "raw-text-content",
                            {text}
                        }
                    }
                }
            } else {
                rsx! {
                    p {
                        strong { "Content (raw bytes): " }
                        code {
                            class: "raw-content-span",
                            {truncate_base64(BASE64_STANDARD.encode(&content), 100)}
                        }
                    }
                }
            }
        }
    }
}

/// Format a QueryReferencesRequest into a human-readable string
fn format_query_references_request(request: &QueryReferencesRequest) -> String {
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
                        output.push_str(&format!("    System: {}\n", BASE64_STANDARD.encode(&system.key)));
                    }
                    if let Some(ref process) = pointer.process {
                        output.push_str(&format!("    Process: {}\n", BASE64_STANDARD.encode(&process.process)));
                    }
                    output.push_str(&format!("    Logical Clock: {}\n", pointer.logical_clock));
                    if let Some(ref digest) = pointer.event_digest {
                        output.push_str(&format!("    Digest Type: {}\n", digest.digest_type));
                        output.push_str(&format!("    Digest: {}\n", BASE64_STANDARD.encode(&digest.digest)));
                    }
                } else {
                    output.push_str(&format!("  Raw: {}\n", BASE64_STANDARD.encode(&reference.reference)));
                }
            },
            3 => {
                // Byte reference (topic)
                if let Ok(topic) = String::from_utf8(reference.reference.clone()) {
                    output.push_str(&format!("  Topic: \"{}\"\n", topic));
                } else {
                    output.push_str(&format!("  Bytes: {}\n", BASE64_STANDARD.encode(&reference.reference)));
                }
            },
            _ => {
                output.push_str(&format!("  Raw: {}\n", BASE64_STANDARD.encode(&reference.reference)));
            }
        }
    }
    
    // Format cursor if present
    if let Some(ref cursor) = request.cursor {
        output.push_str(&format!("\nCursor: {}\n", BASE64_STANDARD.encode(cursor)));
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
                output.push_str(&format!("    Value: {}\n", BASE64_STANDARD.encode(&lww_ref.value)));
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
                output.push_str(&format!("  [{}]: {}\n", i, BASE64_STANDARD.encode(bytes)));
            }
        }
    }
    
    output
}

async fn fetch_api_data() -> Result<QueryReferencesResponse, Box<dyn std::error::Error>> {
    let url = "https://serv1.polycentric.io/query_references?query=CmYIAhJiCiQIARIg1agZt9hNnBewSwAJ4b0HAzP5ujWZBLx43BE6nOYtuvgSEgoQSayWYqLjA0QDdZ3V_tkaWRgHIiQIARIgaWBgT3ALTZXnqqfRfLjAwJJUED_qYwofgA4X8nEhcHcaAggD&moderation_filters=[]";
    
    let response = reqwest::get(url).await?;
    let protobuf_bytes = response.bytes().await?;
    
    // Parse the protobuf directly from the raw bytes
    let query_response = QueryReferencesResponse::decode(&protobuf_bytes[..])?;
    
    Ok(query_response)
}

// Fetch posts by topic
async fn fetch_topic_data(topic: &str) -> Result<QueryReferencesResponse, Box<dyn std::error::Error>> {
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
            class: "container",
            h1 { "Explore" }
            
            button {
                onclick: fetch_data,
                disabled: loading(),
                if loading() { "Loading..." } else { "Fetch Explore Data" }
            }

            if let Some(error) = error_state() {
                p {
                    class: "error-message",
                    strong { "Error: " }
                    {error}
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
            class: "box",
            if let Some(ref result_events) = data.result_events {
                div {
                    h2 { "Result Events ({result_events.events.len()})" }
                    
                    for (index, event) in result_events.events.iter().enumerate() {
                        h4 { "Event {index + 1}" }
                        SignedEventDisplay { event: event.clone() }
                        hr {}
                    }
                }
            }

            if let Some(ref related_events) = data.related_events {
                div {
                    h2 { "Related Events ({related_events.events.len()})" }
                    
                    for (index, event) in related_events.events.iter().enumerate() {
                        h4 { "Related Event {index + 1}" }
                        SignedEventDisplay { event: event.clone() }
                        hr {}
                    }
                }
            }

            if let Some(ref cursor) = data.cursor {
                h3 { "Cursor" }
                pre {
                    class: "mono",
                    {BASE64_STANDARD.encode(cursor)}
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
    let mut username = use_signal(|| String::new());
    let mut current_username = use_signal(|| None::<String>);
    let mut logical_clock = use_signal(|| 1u64);
    let mut status_message = use_signal(|| None::<String>);
    let mut created_post = use_signal(|| None::<SignedEvent>);
    let mut is_posting = use_signal(|| false);
    let mut is_setting_username = use_signal(|| false);
    
    let create_identity = move |_| {
        let (signing_key, public_key) = generate_identity();
        let process = generate_process();
        *identity.write() = Some((signing_key, public_key, process));
        *status_message.write() = Some("Identity created successfully!".to_string());
        *logical_clock.write() = 1;
        *current_username.write() = None;
    };
    
    let set_username = move |_| {
        if let Some((ref signing_key, ref public_key, ref process)) = identity() {
            let username_value = username();
            
            if username_value.is_empty() {
                *status_message.write() = Some("Error: Username cannot be empty".to_string());
                return;
            }
            
            // Clone values for async block
            let signing_key_clone = signing_key.clone();
            let public_key_clone = public_key.clone();
            let process_clone = process.clone();
            let current_clock = logical_clock();
            
            spawn(async move {
                *is_setting_username.write() = true;
                *status_message.write() = Some("Setting username...".to_string());
                
                match create_username(&signing_key_clone, &public_key_clone, &process_clone, current_clock, username_value.clone()) {
                    Ok(signed_event) => {
                        // Try to post to server
                        match post_events_to_server(signed_event.clone()).await {
                            Ok(()) => {
                                *current_username.write() = Some(username_value);
                                *status_message.write() = Some(format!("Username set successfully! Logical clock: {}", current_clock));
                                *logical_clock.write() = current_clock + 1;
                                *username.write() = String::new();
                            }
                            Err(e) => {
                                *status_message.write() = Some(format!("Error posting username to server: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        *status_message.write() = Some(format!("Error creating username event: {}", e));
                    }
                }
                
                *is_setting_username.write() = false;
            });
        } else {
            *status_message.write() = Some("Error: Please create an identity first".to_string());
        }
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
            class: "create-post-container",
            h1 { "Create Post" }
            
            hr {}
            
            // Identity Section
            div {
                class: "identity-section",
                h2 { "Identity" }
                
                if let Some((_, ref public_key, ref process)) = identity() {
                    div {
                        p {
                            class: "identity-detail",
                            strong { "System (Public Key):" }
                            br {}
                            code {
                                class: "identity-key-span",
                                {BASE64_STANDARD.encode(&public_key.key)}
                            }
                        }
                        p {
                            class: "identity-detail",
                            strong { "Process ID:" }
                            br {}
                            code {
                                class: "identity-process-span",
                                {BASE64_STANDARD.encode(&process.process)}
                            }
                        }
                        p {
                            class: "identity-detail",
                            strong { "Logical Clock: " }
                            "{logical_clock()}"
                        }
                        if let Some(ref username_val) = current_username() {
                            div {
                                class: "username-display",
                                strong { "Username: " }
                                span { 
                                    class: "username-value",
                                    "{username_val}" 
                                }
                            }
                        }
                    }
                } else {
                    p { "No identity created yet. Create one to start posting." }
                    button {
                        onclick: create_identity,
                        class: "generate-identity-btn",
                        "Generate Identity"
                    }
                }
            }
            
            hr {}
            
            // Username Section
            if identity().is_some() {
                div {
                    class: "username-section",
                    h2 { "Set Username" }
                    p {
                        class: "topic-query-description",
                        if current_username().is_some() {
                            "Change your username (this will update your identity):"
                        } else {
                            "Set a username for your identity (recommended before posting):"
                        }
                    }
                    
                    label {
                        r#for: "username-input",
                        strong { "Username:" }
                    }
                    br {}
                    input {
                        id: "username-input",
                        r#type: "text",
                        value: "{username}",
                        oninput: move |evt| *username.write() = evt.value(),
                        placeholder: "Enter your username",
                        class: "username-input",
                    }
                    br {}
                    button {
                        onclick: set_username,
                        disabled: username().is_empty() || is_setting_username(),
                        class: "set-username-btn",
                        if is_setting_username() { "Setting..." } else { "Set Username" }
                    }
                }
                
                hr {}
            }
            
            // Post Creation Section
            if identity().is_some() {
                div {
                    class: "post-creation-section",
                    h2 { "Create a Post" }
                    
                    div {
                        class: "form-group",
                        label {
                            class: "form-label",
                            r#for: "topic-input",
                            "Topic (optional)"
                        }
                        input {
                            id: "topic-input",
                            r#type: "text",
                            value: "{topic}",
                            oninput: move |evt| *topic.write() = evt.value(),
                            placeholder: "e.g., technology, music, tamichat",
                            class: "topic-input",
                        }
                        small {
                            class: "help-text",
                            "Posts with topics can be discovered by others interested in the same topic"
                        }
                    }
                    
                    div {
                        class: "form-group",
                        label {
                            class: "form-label",
                            r#for: "post-content",
                            "Post Content"
                        }
                        textarea {
                            id: "post-content",
                            value: "{post_content}",
                            oninput: move |evt| *post_content.write() = evt.value(),
                            placeholder: "What's on your mind?",
                            class: "post-textarea",
                        }
                    }
                    
                    button {
                        onclick: submit_post,
                        disabled: post_content().is_empty() || is_posting(),
                        class: "submit-post-btn",
                        if is_posting() { "Posting..." } else { "Submit Post" }
                    }
                }
                
                hr {}
            }
            
            // Status Messages
            if let Some(ref message) = status_message() {
                p {
                    class: if message.starts_with("Error") { "status-error" } else { "status-success" },
                    {message.clone()}
                }
            }
            
            // Display Created Post
            if let Some(ref post) = created_post() {
                div {
                    class: "created-post-display",
                    h2 { "Last Created Post" }
                    SignedEventDisplay { event: post.clone() }
                }
            }
        }
    }
}
