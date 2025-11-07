use dioxus::prelude::*;
use prost::Message;
use base64::prelude::*;
use ed25519_dalek::SigningKey;

use crate::tamichat::protocol::*;
use crate::api::*;
use crate::storage::*;
use crate::utils::*;

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub content: String,
    pub timestamp: u64,
    pub system_key: String,
}

#[component]
pub fn ChatPage() -> Element {
    let mut messages = use_signal(|| Vec::<ChatMessage>::new());
    let mut message_input = use_signal(|| String::new());
    let mut identity = use_signal(|| None::<(SigningKey, PublicKey, Process, u64)>);
    let mut is_sending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let _show_scroll_button = use_signal(|| false);
    let mut previous_message_count = use_signal(|| 0usize);
    
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
    
    // Set up scroll listener to show/hide button
    use_effect(move || {
        document::eval(&format!(
            r#"
            const messagesDiv = document.getElementById('messages');
            if (messagesDiv) {{
                messagesDiv.addEventListener('scroll', function() {{
                    const isNearBottom = messagesDiv.scrollHeight - messagesDiv.scrollTop - messagesDiv.clientHeight < 100;
                    window.updateScrollButton = !isNearBottom;
                }});
            }}
            "#
        ));
    });
    
    // Auto-create or load identity on mount
    use_effect(move || {
        if identity().is_none() {
            spawn(async move {
                // Try to load existing identity from storage
                match load_identity() {
                    Ok(Some((signing_key, public_key, process, clock))) => {
                        tracing::info!("Loaded existing identity from storage");
                        *identity.write() = Some((signing_key, public_key, process, clock));
                    }
                    Ok(None) => {
                        // No saved identity, create a new one
                        tracing::info!("Creating new identity");
                        let (signing_key, public_key) = generate_identity();
                        let process = generate_process();
                        let clock = 1u64;
                        
                        // Save the new identity
                        if let Err(e) = save_identity(&signing_key, &public_key, &process, clock) {
                            tracing::error!("Failed to save identity: {}", e);
                        }
                        
                        *identity.write() = Some((signing_key, public_key, process, clock));
                    }
                    Err(e) => {
                        tracing::error!("Error loading identity: {}", e);
                        // Fall back to creating a new identity
                        let (signing_key, public_key) = generate_identity();
                        let process = generate_process();
                        *identity.write() = Some((signing_key, public_key, process, 1));
                    }
                }
            });
        }
    });
    
    // Initial fetch
    use_effect(move || {
        spawn(async move {
            if let Err(e) = fetch_and_update_messages(&mut messages, &mut error).await {
                tracing::error!("Error fetching messages: {}", e);
            } else {
                *previous_message_count.write() = messages().len();
                scroll_to_bottom();
            }
        });
    });
    
    // Auto-refresh messages every 5 seconds
    use_effect(move || {
        spawn(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(5_000).await;
                let prev_count = previous_message_count();
                if let Err(e) = fetch_and_update_messages(&mut messages, &mut error).await {
                    tracing::error!("Error auto-fetching messages: {}", e);
                } else {
                    let new_count = messages().len();
                    if new_count > prev_count {
                        // New messages received, scroll to bottom
                        scroll_to_bottom();
                        *previous_message_count.write() = new_count;
                    }
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
                                if let Some((sk, pk, proc, _)) = identity() {
                                    let new_clock = current_clock + 1;
                                    *identity.write() = Some((sk.clone(), pk.clone(), proc.clone(), new_clock));
                                    // Save updated identity to storage
                                    if let Err(e) = save_identity(&sk, &pk, &proc, new_clock) {
                                        tracing::error!("Failed to save identity after post: {}", e);
                                    }
                                }
                                *message_input.write() = String::new();
                                let _ = fetch_and_update_messages(&mut messages, &mut error).await;
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
            class: "chat-messages-wrapper", // New wrapper for positioning
            div {
                class: "chat-messages",
                id: "messages",
                
                if messages().is_empty() {
                    p {
                        class: "loading-indicator",
                        "Loading messages..."
                    }
                }
                div {  
                    class: "messages-list",
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
            }
            
            // Move button outside of scrollable container but inside wrapper
            button {
                class: "scroll-to-bottom-btn",
                onclick: move |_| scroll_to_bottom(),
                title: "Scroll to bottom",
                "↓"
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
                                                            let new_clock = current_clock + 1;
                                                            *identity.write() = Some((sk.clone(), pk.clone(), proc.clone(), new_clock));
                                                            // Save updated identity to storage
                                                            if let Err(e) = save_identity(&sk, &pk, &proc, new_clock) {
                                                                tracing::error!("Failed to save identity after post: {}", e);
                                                            }
                                                        }
                                                        *message_input.write() = String::new();
                                                        let _ = fetch_and_update_messages(&mut messages, &mut error).await;
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
                    if let Ok(parsed_event) = crate::tamichat::protocol::Event::decode(&event.event[..]) {
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

#[component]
pub fn DebugPages(on_back: EventHandler<()>) -> Element {
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
                " "
                button {
                    class: if current_page() == "account" { "debug-btn active" } else { "debug-btn" },
                    onclick: move |_| *current_page.write() = "account",
                    "Account"
                }
            }
            
            match *current_page.read() {
                "query" => rsx! { DataFetcher {} },
                "explore" => rsx! { ExplorePage {} },
                "create" => rsx! { CreatePostPage {} },
                "account" => rsx! { AccountManagement {} },
                _ => rsx! { DataFetcher {} },
            }
        }
    }
}

#[component]
pub fn DataFetcher() -> Element {
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
pub fn DataDisplay(data: QueryReferencesResponse) -> Element {
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
pub fn EventItem(item: QueryReferencesResponseEventItem, index: usize) -> Element {
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
pub fn RelatedEvent(event: SignedEvent, index: usize) -> Element {
    rsx! {
        div {
            class: "box",
            h4 { "Related Event {index + 1}" }
            SignedEventDisplay { event }
        }
    }
}

#[component]
pub fn SignedEventDisplay(event: SignedEvent) -> Element {
    let parsed_event = crate::tamichat::protocol::Event::decode(&event.event[..]).ok();
    
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
pub fn EventDisplay(event: crate::tamichat::protocol::Event) -> Element {
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

            ContentDisplay { content_type: event.content_type, content: event.content }

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
pub fn ContentDisplay(content_type: u64, content: Vec<u8>) -> Element {
    if content.is_empty() {
        return rsx! {
            p {
                class: "no-content-message",
                em { "No content" }
            }
        };
    }

    match content_type {
        3 => {
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
        12 => {
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
        1 => {
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
        11 => {
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

#[component]
pub fn ExplorePage() -> Element {
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
pub fn ExploreDataDisplay(data: ResultEventsAndRelatedEventsAndCursor) -> Element {
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

#[component]
pub fn CreatePostPage() -> Element {
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
    
    // Load identity from storage on mount
    use_effect(move || {
        if identity().is_none() {
            spawn(async move {
                match load_identity() {
                    Ok(Some((signing_key, public_key, process, clock))) => {
                        tracing::info!("Loaded existing identity in CreatePostPage");
                        *identity.write() = Some((signing_key, public_key, process));
                        *logical_clock.write() = clock;
                    }
                    Ok(None) => {
                        tracing::info!("No saved identity found in CreatePostPage");
                    }
                    Err(e) => {
                        tracing::error!("Error loading identity in CreatePostPage: {}", e);
                    }
                }
            });
        }
    });
    
    let create_identity = move |_| {
        let (signing_key, public_key) = generate_identity();
        let process = generate_process();
        let clock = 1u64;
        
        // Save the new identity
        if let Err(e) = save_identity(&signing_key, &public_key, &process, clock) {
            *status_message.write() = Some(format!("Error saving identity: {}", e));
            return;
        }
        
        *identity.write() = Some((signing_key, public_key, process));
        *status_message.write() = Some("Identity created successfully!".to_string());
        *logical_clock.write() = clock;
        *current_username.write() = None;
    };
    
    let set_username = move |_| {
        if let Some((ref signing_key, ref public_key, ref process)) = identity() {
            let username_value = username();
            
            if username_value.is_empty() {
                *status_message.write() = Some("Error: Username cannot be empty".to_string());
                return;
            }
            
            let signing_key_clone = signing_key.clone();
            let public_key_clone = public_key.clone();
            let process_clone = process.clone();
            let current_clock = logical_clock();
            
            spawn(async move {
                *is_setting_username.write() = true;
                *status_message.write() = Some("Setting username...".to_string());
                
                match create_username(&signing_key_clone, &public_key_clone, &process_clone, current_clock, username_value.clone()) {
                    Ok(signed_event) => {
                        match post_events_to_server(signed_event.clone()).await {
                            Ok(()) => {
                                let new_clock = current_clock + 1;
                                *current_username.write() = Some(username_value);
                                *status_message.write() = Some(format!("Username set successfully! Logical clock: {}", current_clock));
                                *logical_clock.write() = new_clock;
                                *username.write() = String::new();
                                // Save updated identity to storage
                                if let Err(e) = save_identity(&signing_key_clone, &public_key_clone, &process_clone, new_clock) {
                                    tracing::error!("Failed to save identity after username: {}", e);
                                }
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
                        match post_events_to_server(signed_event.clone()).await {
                            Ok(()) => {
                                let new_clock = current_clock + 1;
                                *created_post.write() = Some(signed_event.clone());
                                *status_message.write() = Some(format!("Post created and published successfully! Logical clock: {}", current_clock));
                                *logical_clock.write() = new_clock;
                                *post_content.write() = String::new();
                                *topic.write() = String::new();
                                // Save updated identity to storage
                                if let Err(e) = save_identity(&signing_key_clone, &public_key_clone, &process_clone, new_clock) {
                                    tracing::error!("Failed to save identity after post: {}", e);
                                }
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
            
            if let Some(ref message) = status_message() {
                p {
                    class: if message.starts_with("Error") { "status-error" } else { "status-success" },
                    {message.clone()}
                }
            }
            
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

#[component]
pub fn AccountManagement() -> Element {
    let mut identity = use_signal(|| None::<(SigningKey, PublicKey, Process, u64)>);
    let mut status_message = use_signal(|| None::<String>);
    let mut export_data = use_signal(|| None::<String>);
    let mut import_input = use_signal(|| String::new());
    let mut show_import = use_signal(|| false);
    
    // Load identity from storage on mount
    use_effect(move || {
        spawn(async move {
            match load_identity() {
                Ok(Some(loaded_identity)) => {
                    tracing::info!("Loaded existing identity in AccountManagement");
                    *identity.write() = Some(loaded_identity);
                }
                Ok(None) => {
                    tracing::info!("No saved identity found in AccountManagement");
                    *status_message.write() = Some("No identity found. Create one in the 'Create Post' page.".to_string());
                }
                Err(e) => {
                    *status_message.write() = Some(format!("Error loading identity: {}", e));
                }
            }
        });
    });
    
    let delete_account = move |_| {
        match delete_identity() {
            Ok(()) => {
                *identity.write() = None;
                *status_message.write() = Some("Identity deleted successfully. Refresh the page to create a new one.".to_string());
                *export_data.write() = None;
            }
            Err(e) => {
                *status_message.write() = Some(format!("Error deleting identity: {}", e));
            }
        }
    };
    
    let export_account = move |_| {
        if let Some((ref signing_key, ref public_key, ref process, clock)) = identity() {
            match export_identity_json(signing_key, public_key, process, clock) {
                Ok(json) => {
                    *export_data.write() = Some(json);
                    *status_message.write() = Some("Identity exported! Copy the JSON below and save it securely.".to_string());
                }
                Err(e) => {
                    *status_message.write() = Some(format!("Error exporting identity: {}", e));
                }
            }
        } else {
            *status_message.write() = Some("No identity to export.".to_string());
        }
    };
    
    let import_account = move |_| {
        let json = import_input();
        if json.trim().is_empty() {
            *status_message.write() = Some("Error: Import data is empty".to_string());
            return;
        }
        
        match import_identity_json(&json) {
            Ok((signing_key, public_key, process, clock)) => {
                // Save to storage
                match save_identity(&signing_key, &public_key, &process, clock) {
                    Ok(()) => {
                        *identity.write() = Some((signing_key, public_key, process, clock));
                        *status_message.write() = Some("Identity imported successfully!".to_string());
                        *import_input.write() = String::new();
                        *show_import.write() = false;
                        *export_data.write() = None;
                    }
                    Err(e) => {
                        *status_message.write() = Some(format!("Error saving imported identity: {}", e));
                    }
                }
            }
            Err(e) => {
                *status_message.write() = Some(format!("Error importing identity: {}", e));
            }
        }
    };
    
    rsx! {
        div {
            class: "container",
            h1 { "Account Management" }
            
            if let Some(ref message) = status_message() {
                p {
                    class: if message.starts_with("Error") { "status-error" } else { "status-success" },
                    {message.clone()}
                }
            }
            
            hr {}
            
            if let Some((_, ref public_key, ref process, clock)) = identity() {
                div {
                    class: "identity-info",
                    h2 { "Current Identity" }
                    
                    div {
                        class: "identity-detail",
                        strong { "System (Public Key):" }
                        br {}
                        code {
                            class: "identity-key-span",
                            {BASE64_STANDARD.encode(&public_key.key)}
                        }
                    }
                    
                    div {
                        class: "identity-detail",
                        strong { "Process ID:" }
                        br {}
                        code {
                            class: "identity-process-span",
                            {BASE64_STANDARD.encode(&process.process)}
                        }
                    }
                    
                    div {
                        class: "identity-detail",
                        strong { "Logical Clock: " }
                        "{clock}"
                    }
                    
                    hr {}
                    
                    h2 { "Actions" }
                    
                    div {
                        class: "account-actions",
                        button {
                            class: "export-btn",
                            onclick: export_account,
                            "Export Identity"
                        }
                        " "
                        button {
                            class: "import-toggle-btn",
                            onclick: move |_| *show_import.write() = !show_import(),
                            if show_import() { "Hide Import" } else { "Show Import" }
                        }
                        " "
                        button {
                            class: "delete-btn",
                            onclick: delete_account,
                            "Delete Identity"
                        }
                    }
                    
                    if let Some(ref json) = export_data() {
                        div {
                            class: "export-section",
                            h3 { "Exported Identity Data" }
                            p {
                                class: "export-warning",
                                "⚠️ Keep this data secure! Anyone with this JSON can impersonate your identity."
                            }
                            textarea {
                                class: "export-textarea",
                                readonly: true,
                                value: "{json}",
                                rows: 10,
                            }
                        }
                    }
                    
                    if show_import() {
                        div {
                            class: "import-section",
                            h3 { "Import Identity" }
                            p {
                                class: "import-warning",
                                "⚠️ This will replace your current identity! Make sure you've exported your current identity first."
                            }
                            textarea {
                                class: "import-textarea",
                                value: "{import_input}",
                                oninput: move |evt| *import_input.write() = evt.value(),
                                placeholder: "Paste exported identity JSON here...",
                                rows: 10,
                            }
                            br {}
                            button {
                                class: "import-btn",
                                onclick: import_account,
                                disabled: import_input().trim().is_empty(),
                                "Import Identity"
                            }
                        }
                    }
                }
            } else {
                div {
                    class: "no-identity",
                    h2 { "No Identity Found" }
                    p { "You don't have an identity yet. Go to the 'Create Post' page to create one, or import an existing identity below." }
                    
                    hr {}
                    
                    button {
                        class: "import-toggle-btn",
                        onclick: move |_| *show_import.write() = !show_import(),
                        if show_import() { "Hide Import" } else { "Show Import" }
                    }
                    
                    if show_import() {
                        div {
                            class: "import-section",
                            h3 { "Import Identity" }
                            textarea {
                                class: "import-textarea",
                                value: "{import_input}",
                                oninput: move |evt| *import_input.write() = evt.value(),
                                placeholder: "Paste exported identity JSON here...",
                                rows: 10,
                            }
                            br {}
                            button {
                                class: "import-btn",
                                onclick: import_account,
                                disabled: import_input().trim().is_empty(),
                                "Import Identity"
                            }
                        }
                    }
                }
            }
        }
    }
}
