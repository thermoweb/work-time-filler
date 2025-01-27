use rand::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;

// Embedded logo for About screen
const APP_LOGO: &[u8] = include_bytes!("../../../doc/assets/logo.png");

/// Application branding and informational text
/// Extracted from embedded logo metadata
#[derive(Debug, Deserialize)]
pub struct AppBranding {
    #[allow(dead_code)]
    version: String,
    categories: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub secrets: Option<Secrets>,
}

/// Secret data hidden in PNG metadata
#[derive(Debug, Deserialize)]
pub struct Secrets {
    pub sequences: HashMap<String, SecretSequence>,
    pub achievements: HashMap<String, SecretAchievement>,
}

/// A secret key sequence that unlocks an achievement
#[derive(Debug, Deserialize)]
pub struct SecretSequence {
    pub achievement: String,
    pub keys: Vec<String>,
}

/// A secret achievement definition
#[derive(Debug, Deserialize)]
pub struct SecretAchievement {
    pub name: String,
    pub description: String,
    pub icon: String,
    pub chronie_message: String,
}

impl AppBranding {
    /// Load branding information from embedded logo
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let json_str = extract_metadata(APP_LOGO)?;
        let branding: AppBranding = serde_json::from_str(&json_str)?;
        Ok(branding)
    }

    /// Get a random text snippet from a category
    pub fn get_text(&self, category: &str) -> Option<&str> {
        self.categories
            .get(category)
            .and_then(|msgs| msgs.choose(&mut rand::rng()))
            .map(|s| s.as_str())
    }

    /// Get all text snippets from a category
    pub fn get_all(&self, category: &str) -> &[String] {
        self.categories
            .get(category)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get a specific text by index (wraps around)
    pub fn get_by_index(&self, category: &str, index: usize) -> Option<&str> {
        self.categories.get(category).and_then(|msgs| {
            if msgs.is_empty() {
                None
            } else {
                Some(msgs[index % msgs.len()].as_str())
            }
        })
    }
    
    /// Get all available category names (for debugging)
    pub fn get_category_names(&self) -> Vec<&str> {
        self.categories.keys().map(|k| k.as_str()).collect()
    }
}

/// Extract metadata chunk from PNG logo
fn extract_metadata(png_data: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
    // Verify PNG signature
    const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !png_data.starts_with(PNG_SIGNATURE) {
        return Err("Not a valid PNG file".into());
    }

    let mut pos = PNG_SIGNATURE.len();

    // Parse chunks
    while pos + 8 <= png_data.len() {
        // Read chunk length (4 bytes, big-endian)
        let length = u32::from_be_bytes([
            png_data[pos],
            png_data[pos + 1],
            png_data[pos + 2],
            png_data[pos + 3],
        ]) as usize;

        // Read chunk type (4 bytes)
        let chunk_type = &png_data[pos + 4..pos + 8];

        // Check if we have enough data for this chunk
        if pos + 12 + length > png_data.len() {
            break;
        }

        // Check if this is a zTXt chunk
        if chunk_type == b"zTXt" {
            let chunk_data = &png_data[pos + 8..pos + 8 + length];

            // Parse zTXt: keyword\0compression_method\0compressed_data
            if let Some(null_pos) = chunk_data.iter().position(|&b| b == 0) {
                let keyword = std::str::from_utf8(&chunk_data[..null_pos])?;

                if keyword == "chronie_data" {
                    // Check compression method (should be 0 for deflate)
                    if chunk_data.len() > null_pos + 1 && chunk_data[null_pos + 1] == 0 {
                        // Decompress data
                        let compressed_data = &chunk_data[null_pos + 2..];
                        let decompressed = decompress_zlib(compressed_data)?;
                        return Ok(String::from_utf8(decompressed)?);
                    }
                }
            }
        }

        // Move to next chunk (length + type + data + crc)
        pos += 12 + length;
    }

    Err("Branding metadata not found in logo".into())
}

/// Decompress zlib data
fn decompress_zlib(data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;

    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;
    Ok(decompressed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_branding() {
        let branding = AppBranding::load().expect("Failed to load branding");
        assert!(!branding.categories.is_empty(), "No categories loaded");
    }

    #[test]
    fn test_get_text() {
        let branding = AppBranding::load().expect("Failed to load branding");
        let msg = branding.get_text("startup");
        assert!(msg.is_some(), "No startup messages found");
    }

    #[test]
    fn test_get_all() {
        let branding = AppBranding::load().expect("Failed to load branding");
        let startup_msgs = branding.get_all("startup");
        assert!(!startup_msgs.is_empty(), "No startup messages found");
    }
    
    #[test]
    fn test_secrets_load() {
        let branding = AppBranding::load().expect("Failed to load branding");
        assert!(branding.secrets.is_some(), "No secrets found in PNG");
        
        let secrets = branding.secrets.as_ref().unwrap();
        
        // Verify chronie sequence exists
        assert!(secrets.sequences.contains_key("chronie"), "Chronie sequence not found");
        let chronie_seq = &secrets.sequences["chronie"];
        assert_eq!(chronie_seq.keys, vec!["c", "h", "r", "o", "n", "i", "e"]);
        assert_eq!(chronie_seq.achievement, "secret_chronie_friend");
        
        // Verify achievement exists
        assert!(secrets.achievements.contains_key("secret_chronie_friend"), "Chronie achievement not found");
        let achievement = &secrets.achievements["secret_chronie_friend"];
        assert_eq!(achievement.name, "Chronie's Friend");
        assert_eq!(achievement.icon, "🧙");
        assert!(!achievement.chronie_message.is_empty());
        
        println!("✅ Secret sequence: {:?}", chronie_seq.keys);
        println!("✅ Secret achievement: {} {}", achievement.icon, achievement.name);
        println!("✅ Chronie says: {}", achievement.chronie_message);
    }
}
