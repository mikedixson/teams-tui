//! Image display module for Teams TUI
//!
//! This module provides support for displaying images in the terminal using
//! the Kitty graphics protocol (and fallbacks like Sixel, iTerm2, or halfblocks).
//!
//! The implementation uses the `ratatui-image` crate which handles protocol
//! detection and rendering automatically.

use anyhow::{Context, Result};
use image::DynamicImage;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use std::collections::HashMap;
use std::io::{stdout, Write};

/// Image picker for creating image protocols
/// This is initialized once at startup by querying the terminal for
/// its capabilities and font size.
pub struct ImagePicker {
    picker: Picker,
}

impl ImagePicker {
    /// Create a new ImagePicker by querying the terminal for capabilities
    pub fn new() -> Result<Self> {
        // Try to query the terminal for font size and protocol support
        // This will detect Kitty, Sixel, iTerm2, or fall back to halfblocks
        let picker = Picker::from_query_stdio()
            .context("Failed to query terminal for image protocol support")?;

        Ok(Self { picker })
    }

    /// Create a new ImagePicker with a fallback font size
    /// Use this if the terminal query fails
    pub fn with_fallback_fontsize() -> Self {
        // Default font size (8x12 pixels per character cell is common)
        let picker = Picker::from_fontsize((8, 12));
        Self { picker }
    }

    /// Get the detected protocol type
    pub fn protocol_type(&self) -> ProtocolType {
        self.picker.protocol_type()
    }

    /// Check if the terminal supports any graphics protocol (not just halfblocks)
    pub fn supports_graphics(&self) -> bool {
        matches!(
            self.picker.protocol_type(),
            ProtocolType::Kitty | ProtocolType::Sixel | ProtocolType::Iterm2
        )
    }

    /// Create a new resize protocol for an image
    /// This prepares the image for rendering with automatic resizing
    pub fn new_resize_protocol(&mut self, image: DynamicImage) -> StatefulProtocol {
        self.picker.new_resize_protocol(image)
    }
}

/// Cache for loaded images
/// This stores downloaded and decoded images to avoid re-downloading
pub struct ImageCache {
    /// Map from URL to decoded image
    images: HashMap<String, DynamicImage>,
    /// Maximum number of images to cache
    max_size: usize,
}

impl ImageCache {
    /// Create a new image cache with the given maximum size
    pub fn new(max_size: usize) -> Self {
        Self {
            images: HashMap::new(),
            max_size,
        }
    }

    /// Get an image from the cache
    pub fn get(&self, url: &str) -> Option<&DynamicImage> {
        self.images.get(url)
    }

    /// Insert an image into the cache
    pub fn insert(&mut self, url: String, image: DynamicImage) {
        // Simple cache eviction: remove oldest entries if over capacity
        if self.images.len() >= self.max_size {
            // Remove first entry (not ideal but simple)
            if let Some(key) = self.images.keys().next().cloned() {
                self.images.remove(&key);
            }
        }
        self.images.insert(url, image);
    }

    /// Check if an image is in the cache
    pub fn contains(&self, url: &str) -> bool {
        self.images.contains_key(url)
    }

    /// Clear the cache
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.images.clear();
    }
}

/// Load an image from bytes
pub fn load_image_from_bytes(bytes: &[u8]) -> Result<DynamicImage> {
    let image = image::load_from_memory(bytes)
        .context("Failed to decode image")?;
    Ok(image)
}

/// Download an image from a URL using the provided access token
pub async fn download_image(
    client: &reqwest::Client,
    url: &str,
    access_token: &str,
) -> Result<Vec<u8>> {
    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .context("Failed to send image request")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to download image: {}", response.status());
    }

    let bytes = response
        .bytes()
        .await
        .context("Failed to read image bytes")?;

    Ok(bytes.to_vec())
}

/// Print information about the detected image protocol
pub fn print_protocol_info(picker: &ImagePicker) {
    let protocol = picker.protocol_type();
    let protocol_name = match protocol {
        ProtocolType::Kitty => "Kitty",
        ProtocolType::Sixel => "Sixel",
        ProtocolType::Iterm2 => "iTerm2",
        ProtocolType::Halfblocks => "Halfblocks (fallback)",
    };

    println!("Image protocol: {}", protocol_name);
    if picker.supports_graphics() {
        println!("✓ Full graphics support available");
    } else {
        println!("⚠ Using Unicode halfblock fallback (limited image quality)");
    }
    stdout().flush().ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_cache_basic() {
        let mut cache = ImageCache::new(2);
        
        // Create a simple 1x1 test image
        let img = DynamicImage::new_rgb8(1, 1);
        
        assert!(!cache.contains("test1"));
        cache.insert("test1".to_string(), img.clone());
        assert!(cache.contains("test1"));
        assert!(cache.get("test1").is_some());
    }

    #[test]
    fn test_image_cache_eviction() {
        let mut cache = ImageCache::new(2);
        let img = DynamicImage::new_rgb8(1, 1);
        
        cache.insert("img1".to_string(), img.clone());
        cache.insert("img2".to_string(), img.clone());
        assert_eq!(cache.images.len(), 2);
        
        // This should trigger eviction
        cache.insert("img3".to_string(), img.clone());
        assert_eq!(cache.images.len(), 2);
    }

    #[test]
    fn test_load_image_from_bytes() {
        // Create a minimal valid PNG
        let png_bytes = include_bytes!("../assets/images/tt.png");
        let result = load_image_from_bytes(png_bytes);
        assert!(result.is_ok());
    }
}
