use dioxus::prelude::*;

pub mod tamichat {
    pub mod protocol {
        include!(concat!(env!("OUT_DIR"), "/tamichat.protocol.rs"));
    }
}

mod api;
mod components;
mod utils;

use components::{ChatPage, DebugPages};

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");

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
                "Debug"
            }
        }
    }
}
