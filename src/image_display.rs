//! Image display module for Teams TUI
//!
//! This module provides support for displaying images in the terminal using
//! the Kitty graphics protocol (and fallbacks like Sixel, iTerm2, or halfblocks).
//!
//! The implementation uses the `ratatui-image` crate which handles protocol
//! detection and rendering automatically.

use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use image::DynamicImage;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use serde::Deserialize;
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
    /// Note: When capacity is exceeded, an arbitrary entry is removed (not necessarily oldest)
    /// since HashMap doesn't maintain insertion order.
    pub fn insert(&mut self, url: String, image: DynamicImage) {
        // Simple cache eviction: remove an arbitrary entry if over capacity
        if self.images.len() >= self.max_size {
            // Remove an arbitrary entry (HashMap iteration order is not guaranteed)
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
    let image = image::load_from_memory(bytes).context("Failed to decode image")?;
    Ok(image)
}

/// Response from the Graph API shares endpoint
#[derive(Debug, Deserialize)]
struct SharesResponse {
    #[serde(rename = "@microsoft.graph.downloadUrl")]
    download_url: Option<String>,
}

/// Convert a SharePoint/OneDrive URL to a Graph API shares endpoint URL
fn url_to_shares_endpoint(url: &str) -> String {
    // Encode the URL in base64 for the shares endpoint
    // See: https://learn.microsoft.com/en-us/graph/api/shares-get
    let encoded = URL_SAFE_NO_PAD.encode(url);
    format!(
        "https://graph.microsoft.com/v1.0/shares/u!{}/driveItem",
        encoded
    )
}

/// Download an image from a URL using the provided access token
///
/// Teams uses different URL patterns for images:
/// - Graph API URLs: Direct access with Bearer token
/// - SharePoint/OneDrive URLs: Uses Graph API shares endpoint to get download URL
/// - Hosted content: Inline images embedded in messages
pub async fn download_image(
    client: &reqwest::Client,
    url: &str,
    access_token: &str,
) -> Result<Vec<u8>> {
    let url_lower = url.to_lowercase();

    // For SharePoint/OneDrive URLs, use the Graph API shares endpoint
    if url_lower.contains("sharepoint.com") || url_lower.contains("onedrive") {
        return download_sharepoint_image(client, url, access_token).await;
    }

    // For other URLs (Graph API, etc.), try direct access with Bearer token
    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .context("Failed to send image request")?;

    let status = response.status();

    if status.is_success() {
        let bytes = response
            .bytes()
            .await
            .context("Failed to read image bytes")?;
        return Ok(bytes.to_vec());
    }

    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        if url_lower.contains("graph.microsoft.com") {
            anyhow::bail!(
                "Graph API access denied ({}). Token may have expired - try deleting ~/.config/teams-tui/token.json and restart.",
                status
            );
        } else {
            anyhow::bail!(
                "Access denied ({}) - URL may require additional permissions",
                status
            );
        }
    }

    anyhow::bail!("Failed to download image: {}", status)
}

/// Download an image from SharePoint/OneDrive using the Graph API shares endpoint
async fn download_sharepoint_image(
    client: &reqwest::Client,
    sharepoint_url: &str,
    access_token: &str,
) -> Result<Vec<u8>> {
    // Step 1: Use the shares endpoint to get the driveItem with download URL
    let shares_url = url_to_shares_endpoint(sharepoint_url);

    let response = client
        .get(&shares_url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .context("Failed to query Graph API shares endpoint")?;

    let status = response.status();

    if !status.is_success() {
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            anyhow::bail!(
                "Cannot access SharePoint file via Graph API ({}). \
                Make sure Files.Read.All or Sites.Read.All permission is granted and re-authenticate.",
                status
            );
        }
        anyhow::bail!("Graph API shares endpoint returned error: {}", status);
    }

    // Parse the response to get the download URL
    let shares_response: SharesResponse = response
        .json()
        .await
        .context("Failed to parse shares response")?;

    let download_url = shares_response.download_url.ok_or_else(|| {
        anyhow::anyhow!("No download URL in shares response - file may not be accessible")
    })?;

    // Step 2: Download the actual file content using the temporary download URL
    // Note: The download URL is pre-authenticated and doesn't need a Bearer token
    let file_response = client
        .get(&download_url)
        .send()
        .await
        .context("Failed to download file from SharePoint")?;

    if !file_response.status().is_success() {
        anyhow::bail!(
            "Failed to download file from SharePoint: {}",
            file_response.status()
        );
    }

    let bytes = file_response
        .bytes()
        .await
        .context("Failed to read file bytes")?;

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
