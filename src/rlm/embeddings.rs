//! Embedding providers for RLM chunk pre-scoring
//!
//! Supports:
//! - OpenAI text-embedding-3-small (default if OPENAI_API_KEY set)
//! - Local fastembed (offline fallback)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use tracing::{info, warn};

/// Trait for embedding providers
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embeddings for multiple texts
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    /// Get the embedding dimension
    fn dimension(&self) -> usize;
}

/// OpenAI embedding provider
pub struct OpenAIEmbedder {
    api_key: String,
    model: String,
    client: reqwest::blocking::Client,
}

/// Response from OpenAI embeddings API
#[derive(Debug, Deserialize)]
struct OpenAIEmbeddingResponse {
    data: Vec<OpenAIEmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct OpenAIEmbeddingData {
    embedding: Vec<f32>,
}

/// Request to OpenAI embeddings API
#[derive(Debug, Serialize)]
struct OpenAIEmbeddingRequest {
    model: String,
    input: Vec<String>,
}

impl OpenAIEmbedder {
    /// Create a new OpenAI embedder
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            model: "text-embedding-3-small".to_string(),
            client: reqwest::blocking::Client::new(),
        }
    }

    /// Create with custom model
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

impl EmbeddingProvider for OpenAIEmbedder {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let request = OpenAIEmbeddingRequest {
            model: self.model.clone(),
            input: texts.iter().map(|s| s.to_string()).collect(),
        };

        let response = self
            .client
            .post("https://api.openai.com/v1/embeddings")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .context("Failed to call OpenAI embeddings API")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().unwrap_or_default();
            anyhow::bail!("OpenAI API error ({}): {}", status, text);
        }

        let result: OpenAIEmbeddingResponse =
            response.json().context("Failed to parse OpenAI response")?;

        Ok(result.data.into_iter().map(|d| d.embedding).collect())
    }

    fn dimension(&self) -> usize {
        1536 // text-embedding-3-small dimension
    }
}

/// Local embedding provider using fastembed (stub for now)
pub struct LocalEmbedder {
    dimension: usize,
}

impl Default for LocalEmbedder {
    fn default() -> Self {
        Self { dimension: 384 } // Default fastembed dimension
    }
}

impl EmbeddingProvider for LocalEmbedder {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        // Stub implementation - returns random-ish vectors based on text hash
        warn!("Using stub local embedder - implement with fastembed for production");

        Ok(texts
            .iter()
            .map(|text| {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                let mut hasher = DefaultHasher::new();
                text.hash(&mut hasher);
                let seed = hasher.finish();

                // Generate deterministic pseudo-random vector from hash
                let mut rng_state = seed;
                (0..self.dimension)
                    .map(|_| {
                        rng_state = rng_state
                            .wrapping_mul(6364136223846793005)
                            .wrapping_add(1442695040888963407);
                        ((rng_state >> 33) as f32 / u32::MAX as f32) * 2.0 - 1.0
                    })
                    .collect()
            })
            .collect())
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

/// Get the default embedding provider based on environment
pub fn get_default_embedder() -> Result<Box<dyn EmbeddingProvider>> {
    // Try OpenAI first
    if let Ok(api_key) = env::var("OPENAI_API_KEY") {
        if !api_key.is_empty() {
            info!("Using OpenAI embeddings");
            return Ok(Box::new(OpenAIEmbedder::new(api_key)));
        }
    }

    // Fall back to local
    info!("Using local embeddings (stub - implement fastembed for production)");
    Ok(Box::new(LocalEmbedder::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_embedder() {
        let embedder = LocalEmbedder::default();
        let texts = vec!["hello world", "test text"];
        let embeddings = embedder.embed(&texts).unwrap();

        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), embedder.dimension());

        // Same text should give same embedding
        let same = embedder.embed(&["hello world"]).unwrap();
        assert_eq!(embeddings[0], same[0]);
    }

    #[test]
    fn test_local_embedder_empty() {
        let embedder = LocalEmbedder::default();
        let embeddings = embedder.embed(&[]).unwrap();
        assert!(embeddings.is_empty());
    }

    #[test]
    fn test_local_embedder_dimension() {
        let embedder = LocalEmbedder::default();
        assert_eq!(embedder.dimension(), 384);
    }

    #[test]
    fn test_local_embedder_deterministic() {
        let embedder = LocalEmbedder::default();
        let text = "consistent test";
        let emb1 = embedder.embed(&[text]).unwrap();
        let emb2 = embedder.embed(&[text]).unwrap();
        assert_eq!(emb1[0], emb2[0]);
    }

    #[test]
    fn test_local_embedder_different_texts() {
        let embedder = LocalEmbedder::default();
        let emb1 = embedder.embed(&["text one"]).unwrap();
        let emb2 = embedder.embed(&["text two"]).unwrap();
        // Different texts should produce different embeddings
        assert_ne!(emb1[0], emb2[0]);
    }

    #[test]
    fn test_get_default_embedder() {
        // Should not fail even without API key
        let embedder = get_default_embedder().unwrap();
        assert!(embedder.dimension() > 0);
    }

    #[test]
    fn test_openai_embedder_creation() {
        let embedder = OpenAIEmbedder::new("test-key".to_string());
        assert_eq!(embedder.dimension(), 1536);
    }

    #[test]
    fn test_openai_embedder_with_model() {
        let embedder =
            OpenAIEmbedder::new("test-key".to_string()).with_model("text-embedding-ada-002");
        assert_eq!(embedder.model, "text-embedding-ada-002");
    }
}
