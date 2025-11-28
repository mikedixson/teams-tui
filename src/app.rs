use crate::api::{Chat, Message};
use crate::image_display::{ImageCache, ImagePicker};
use ratatui_image::protocol::StatefulProtocol;
use std::collections::HashMap;

pub struct App {
    pub chats: Vec<Chat>,
    pub status: String,
    pub selected_index: usize,
    pub current_user_name: Option<String>,
    pub messages: Vec<Message>,
    pub loading_messages: bool,
    pub input_mode: bool,
    pub input_buffer: String,
    pub scroll_offset: u16,
    pub max_scroll: u16,
    pub snap_to_bottom: bool,
    /// Image picker for creating image protocols (optional, may fail on unsupported terminals)
    pub image_picker: Option<ImagePicker>,
    /// Cache for downloaded images
    pub image_cache: ImageCache,
    /// Prepared image protocols ready for rendering (keyed by attachment URL)
    pub image_protocols: HashMap<String, StatefulProtocol>,
}

impl App {
    pub fn new() -> App {
        // Try to create image picker, but don't fail if terminal doesn't support it
        let image_picker = match ImagePicker::new() {
            Ok(picker) => Some(picker),
            Err(_) => {
                // Fall back to a picker with default font size
                Some(ImagePicker::with_fallback_fontsize())
            }
        };

        App {
            chats: Vec::new(),
            status: "Loading...".to_string(),
            selected_index: 0,
            current_user_name: None,
            messages: Vec::new(),
            loading_messages: false,
            input_mode: false,
            input_buffer: String::new(),
            scroll_offset: 0,
            max_scroll: 0,
            snap_to_bottom: true,
            image_picker,
            image_cache: ImageCache::new(50), // Cache up to 50 images
            image_protocols: HashMap::new(),
        }
    }

    pub fn set_chats(&mut self, chats: Vec<Chat>) {
        self.chats = chats;
        self.status = format!("Loaded {} chats", self.chats.len());
    }

    pub fn set_current_user(&mut self, name: String) {
        self.current_user_name = Some(name);
    }

    pub fn set_messages(&mut self, messages: Vec<Message>) {
        self.messages = messages;
        self.loading_messages = false;
    }

    pub fn set_loading_messages(&mut self, loading: bool) {
        self.loading_messages = loading;
    }

    pub fn get_selected_chat(&self) -> Option<&Chat> {
        self.chats.get(self.selected_index)
    }

    pub fn next_chat(&mut self) {
        if !self.chats.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.chats.len();
            // Clear image protocols when changing chats
            self.image_protocols.clear();
        }
    }

    pub fn previous_chat(&mut self) {
        if !self.chats.is_empty() {
            if self.selected_index > 0 {
                self.selected_index -= 1;
            } else {
                self.selected_index = self.chats.len() - 1;
            }
            // Clear image protocols when changing chats
            self.image_protocols.clear();
        }
    }

    /// Check if the terminal supports graphics protocols (Kitty, Sixel, iTerm2)
    pub fn supports_graphics(&self) -> bool {
        self.image_picker.as_ref().map_or(false, |p| p.supports_graphics())
    }

    /// Prepare an image for rendering by creating a protocol
    pub fn prepare_image(&mut self, url: &str, image: image::DynamicImage) {
        if let Some(ref mut picker) = self.image_picker {
            let protocol = picker.new_resize_protocol(image);
            self.image_protocols.insert(url.to_string(), protocol);
        }
    }

    /// Check if an image is ready for rendering
    pub fn has_prepared_image(&self, url: &str) -> bool {
        self.image_protocols.contains_key(url)
    }
}
