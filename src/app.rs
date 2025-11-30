use crate::api::{Chat, Message};
use crate::image_display::{ImageCache, ImagePicker};
use ratatui::layout::Rect;
use ratatui_image::protocol::StatefulProtocol;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ActivePane {
    ChatList,
    Messages,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FocusedPane {
    ChatList,
    Messages,
}

/// Represents an image that can be viewed
#[derive(Clone)]
pub struct ViewableImage {
    pub name: String,
    pub url: String,
}

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
    pub active_pane: ActivePane,
    pub focused_pane: FocusedPane,
    pub chat_list_area: Rect,
    pub messages_area: Rect,
    /// Image picker for creating image protocols (optional, may fail on unsupported terminals)
    pub image_picker: Option<ImagePicker>,
    /// Cache for downloaded images
    pub image_cache: ImageCache,
    /// Prepared image protocols ready for rendering (keyed by attachment URL)
    pub image_protocols: HashMap<String, StatefulProtocol>,
    /// Image viewing mode - when Some, display the image viewer
    pub viewing_image: Option<ViewableImage>,
    /// Current image protocol for viewing
    pub current_image_protocol: Option<StatefulProtocol>,
    /// Whether we're currently loading an image
    pub loading_image: bool,
    /// Error message for image loading (persists until cleared)
    pub image_error: Option<String>,
    /// List of viewable images in current messages
    pub viewable_images: Vec<ViewableImage>,
    /// Index of currently selected/viewing image
    pub selected_image_index: usize,
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
            active_pane: ActivePane::ChatList,
            focused_pane: FocusedPane::ChatList,
            chat_list_area: Rect::default(),
            messages_area: Rect::default(),
            image_picker,
            image_cache: ImageCache::new(50), // Cache up to 50 images
            image_protocols: HashMap::new(),
            viewing_image: None,
            current_image_protocol: None,
            loading_image: false,
            image_error: None,
            viewable_images: Vec::new(),
            selected_image_index: 0,
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
        // Update viewable images list
        self.update_viewable_images();
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
            self.viewable_images.clear();
            self.selected_image_index = 0;
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
            self.viewable_images.clear();
            self.selected_image_index = 0;
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

    /// Update the list of viewable images from current messages
    fn update_viewable_images(&mut self) {
        self.viewable_images.clear();
        for msg in &self.messages {
            for attachment in &msg.attachments {
                if attachment.is_image() {
                    if let Some(url) = attachment.get_image_url() {
                        self.viewable_images.push(ViewableImage {
                            name: attachment.name.clone().unwrap_or_else(|| "image".to_string()),
                            url: url.to_string(),
                        });
                    }
                }
            }
        }
        self.selected_image_index = 0;
    }

    /// Check if we're currently viewing an image
    pub fn is_viewing_image(&self) -> bool {
        self.viewing_image.is_some()
    }

    /// Start viewing an image
    pub fn start_viewing_image(&mut self, image: ViewableImage) {
        self.status = format!("Loading image: {}...", image.name);
        self.viewing_image = Some(image);
        self.loading_image = true;
        self.current_image_protocol = None;
        self.image_error = None; // Clear any previous error
    }

    /// Set the loaded image protocol for viewing
    pub fn set_image_protocol(&mut self, protocol: StatefulProtocol) {
        self.current_image_protocol = Some(protocol);
        self.loading_image = false;
        self.image_error = None;
    }

    /// Set an image loading error
    pub fn set_image_error(&mut self, error: String) {
        self.loading_image = false;
        self.image_error = Some(error);
    }

    /// Stop viewing the current image
    pub fn stop_viewing_image(&mut self) {
        self.viewing_image = None;
        self.current_image_protocol = None;
        self.loading_image = false;
        self.image_error = None;
    }

    /// Get the current viewable image if any
    pub fn get_current_viewable_image(&self) -> Option<&ViewableImage> {
        if self.viewable_images.is_empty() {
            None
        } else {
            self.viewable_images.get(self.selected_image_index)
        }
    }

    /// Move to next image in the list
    pub fn next_image(&mut self) {
        if !self.viewable_images.is_empty() {
            self.selected_image_index = (self.selected_image_index + 1) % self.viewable_images.len();
        }
    }

    /// Move to previous image in the list
    pub fn previous_image(&mut self) {
        if !self.viewable_images.is_empty() {
            if self.selected_image_index > 0 {
                self.selected_image_index -= 1;
            } else {
                self.selected_image_index = self.viewable_images.len() - 1;
            }
        }
    }
}
