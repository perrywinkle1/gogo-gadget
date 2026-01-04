//! Content chunking strategies for RLM

use crate::rlm::types::{Chunk, ChunkMetadata, StructuralType, estimate_tokens};
use anyhow::{Result, Context};
use std::path::Path;
use std::fs;
use tracing::{debug, warn};

/// Strategy for chunking content
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkStrategy {
    /// Split at structural code boundaries (functions, classes, etc.)
    Structural,

    /// Split at semantic/topic boundaries
    Semantic,

    /// Fixed-size chunks with overlap
    FixedSize { size: usize, overlap: usize },

    /// Split by file boundaries (for directories)
    ByFile,

    /// Hierarchical outline-based splitting
    Hierarchical,
}

impl Default for ChunkStrategy {
    fn default() -> Self {
        ChunkStrategy::FixedSize { size: 4000, overlap: 200 }
    }
}

/// Trait for content chunkers
pub trait ContextChunker: Send + Sync {
    /// Chunk content using the specified strategy
    fn chunk(&self, content: &str, strategy: ChunkStrategy) -> Result<Vec<Chunk>>;

    /// Chunk a file
    fn chunk_file(&self, path: &Path, strategy: ChunkStrategy) -> Result<Vec<Chunk>>;

    /// Chunk a directory
    fn chunk_directory(&self, path: &Path, strategy: ChunkStrategy) -> Result<Vec<Chunk>>;

    /// Estimate tokens in content
    fn estimate_tokens(&self, content: &str) -> u32;
}

/// Default chunker implementation
pub struct DefaultChunker {
    /// Target chunk size in characters (approximate tokens * 4)
    target_size: usize,

    /// Overlap between chunks
    overlap: usize,

    /// Maximum file size to process (skip larger files)
    max_file_size: usize,

    /// File extensions to include
    include_extensions: Vec<String>,
}

impl Default for DefaultChunker {
    fn default() -> Self {
        Self {
            target_size: 16000, // ~4000 tokens * 4 chars
            overlap: 800,       // ~200 tokens * 4 chars
            max_file_size: 10 * 1024 * 1024, // 10MB
            include_extensions: vec![
                "rs", "py", "js", "ts", "tsx", "jsx",
                "go", "java", "c", "cpp", "h", "hpp",
                "rb", "php", "swift", "kt", "scala",
                "md", "txt", "json", "yaml", "yml", "toml",
            ].into_iter().map(String::from).collect(),
        }
    }
}

impl DefaultChunker {
    /// Create a new chunker with custom settings
    pub fn new(target_size: usize, overlap: usize) -> Self {
        Self {
            target_size,
            overlap,
            ..Default::default()
        }
    }

    /// Check if a file should be included
    fn should_include_file(&self, path: &Path) -> bool {
        // Skip hidden files
        if path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with('.'))
            .unwrap_or(false)
        {
            return false;
        }

        // Check extension
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            self.include_extensions.iter().any(|e| e == ext)
        } else {
            false
        }
    }

    /// Detect if content is likely binary
    fn is_binary(content: &[u8]) -> bool {
        // Check first 8KB for null bytes
        let check_len = content.len().min(8192);
        content[..check_len].iter().any(|&b| b == 0)
    }

    /// Detect programming language from file extension
    fn detect_language(path: &Path) -> Option<String> {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|ext| match ext {
                "rs" => "rust",
                "py" => "python",
                "js" | "jsx" => "javascript",
                "ts" | "tsx" => "typescript",
                "go" => "go",
                "java" => "java",
                "c" | "h" => "c",
                "cpp" | "hpp" | "cc" => "cpp",
                "rb" => "ruby",
                "php" => "php",
                "swift" => "swift",
                "kt" => "kotlin",
                "md" => "markdown",
                _ => ext,
            })
            .map(String::from)
    }

    /// Find structural boundaries in code
    fn find_structural_boundaries(&self, content: &str, language: Option<&str>) -> Vec<(usize, StructuralType)> {
        let mut boundaries = Vec::new();

        let patterns: Vec<(&str, StructuralType)> = match language {
            Some("rust") => vec![
                (r"(?m)^pub fn |^fn |^async fn ", StructuralType::Function),
                (r"(?m)^pub struct |^struct ", StructuralType::Struct),
                (r"(?m)^pub enum |^enum ", StructuralType::Enum),
                (r"(?m)^pub trait |^trait ", StructuralType::Trait),
                (r"(?m)^impl ", StructuralType::Impl),
                (r"(?m)^pub mod |^mod ", StructuralType::Module),
            ],
            Some("python") => vec![
                (r"(?m)^def |^async def ", StructuralType::Function),
                (r"(?m)^class ", StructuralType::Class),
            ],
            Some("javascript") | Some("typescript") => vec![
                (r"(?m)^function |^async function |^export function ", StructuralType::Function),
                (r"(?m)^class |^export class ", StructuralType::Class),
                (r"(?m)^interface |^export interface ", StructuralType::Interface),
            ],
            Some("go") => vec![
                (r"(?m)^func ", StructuralType::Function),
                (r"(?m)^type \w+ struct", StructuralType::Struct),
                (r"(?m)^type \w+ interface", StructuralType::Interface),
            ],
            _ => vec![],
        };

        for (pattern, struct_type) in patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                for m in re.find_iter(content) {
                    boundaries.push((m.start(), struct_type));
                }
            }
        }

        boundaries.sort_by_key(|(pos, _)| *pos);
        boundaries
    }

    /// Chunk using fixed-size strategy
    fn chunk_fixed_size(&self, content: &str, size: usize, overlap: usize) -> Vec<Chunk> {
        let mut chunks = Vec::new();
        let mut chunk_id = 0;
        let mut start = 0;

        while start < content.len() {
            let mut end = (start + size).min(content.len());

            // Try to break at a good point (newline, period, space)
            if end < content.len() {
                // Look for newline first
                if let Some(pos) = content[start..end].rfind('\n') {
                    end = start + pos + 1;
                } else if let Some(pos) = content[start..end].rfind(". ") {
                    end = start + pos + 2;
                } else if let Some(pos) = content[start..end].rfind(' ') {
                    end = start + pos + 1;
                }
            }

            // Ensure we're at a valid char boundary
            while end < content.len() && !content.is_char_boundary(end) {
                end += 1;
            }

            let chunk_content = &content[start..end];
            if !chunk_content.trim().is_empty() {
                chunks.push(Chunk::new(
                    format!("chunk-{}", chunk_id),
                    chunk_content.to_string(),
                ).with_byte_range(start, end));
                chunk_id += 1;
            }

            // Move start with overlap
            if end >= content.len() {
                break;
            }
            start = end.saturating_sub(overlap);
            while start < content.len() && !content.is_char_boundary(start) {
                start += 1;
            }
        }

        chunks
    }

    /// Chunk using structural boundaries
    fn chunk_structural(&self, content: &str, language: Option<&str>) -> Vec<Chunk> {
        let boundaries = self.find_structural_boundaries(content, language);

        if boundaries.is_empty() {
            // Fall back to fixed-size
            return self.chunk_fixed_size(content, self.target_size, self.overlap);
        }

        let mut chunks = Vec::new();

        for (i, (pos, struct_type)) in boundaries.iter().enumerate() {
            // Get content from previous boundary to this one
            let end = if i + 1 < boundaries.len() {
                boundaries[i + 1].0
            } else {
                content.len()
            };

            let chunk_content = &content[*pos..end];
            if !chunk_content.trim().is_empty() {
                let mut chunk = Chunk::new(
                    format!("struct-{}", i),
                    chunk_content.to_string(),
                ).with_byte_range(*pos, end);

                chunk.metadata = ChunkMetadata {
                    language: language.map(String::from),
                    is_code: true,
                    is_structural_boundary: true,
                    structural_type: Some(*struct_type),
                    created_at: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                };

                // If chunk is too large, split it further
                if chunk.token_count > self.target_size as u32 / 4 {
                    let sub_chunks = self.chunk_fixed_size(chunk_content, self.target_size, self.overlap);
                    for (j, mut sub) in sub_chunks.into_iter().enumerate() {
                        sub.id = format!("struct-{}-{}", i, j);
                        sub.metadata = chunk.metadata.clone();
                        chunks.push(sub);
                    }
                } else {
                    chunks.push(chunk);
                }
            }
        }

        chunks
    }
}

impl ContextChunker for DefaultChunker {
    fn chunk(&self, content: &str, strategy: ChunkStrategy) -> Result<Vec<Chunk>> {
        if content.is_empty() {
            return Ok(Vec::new());
        }

        match strategy {
            ChunkStrategy::FixedSize { size, overlap } => {
                Ok(self.chunk_fixed_size(content, size * 4, overlap * 4)) // Convert tokens to chars
            }
            ChunkStrategy::Structural => {
                Ok(self.chunk_structural(content, None))
            }
            ChunkStrategy::Semantic | ChunkStrategy::Hierarchical => {
                // Fall back to fixed-size for now
                Ok(self.chunk_fixed_size(content, self.target_size, self.overlap))
            }
            ChunkStrategy::ByFile => {
                // For plain content, treat as single chunk or use fixed-size
                if estimate_tokens(content) <= self.target_size as u32 / 4 {
                    Ok(vec![Chunk::new("file-0", content.to_string())])
                } else {
                    Ok(self.chunk_fixed_size(content, self.target_size, self.overlap))
                }
            }
        }
    }

    fn chunk_file(&self, path: &Path, strategy: ChunkStrategy) -> Result<Vec<Chunk>> {
        let metadata = fs::metadata(path)
            .context("Failed to read file metadata")?;

        if metadata.len() > self.max_file_size as u64 {
            warn!("Skipping large file: {} ({} bytes)", path.display(), metadata.len());
            return Ok(Vec::new());
        }

        let bytes = fs::read(path)
            .context("Failed to read file")?;

        if Self::is_binary(&bytes) {
            debug!("Skipping binary file: {}", path.display());
            return Ok(Vec::new());
        }

        let content = String::from_utf8_lossy(&bytes);
        let language = Self::detect_language(path);

        let strategy = if strategy == ChunkStrategy::Structural && language.is_some() {
            ChunkStrategy::Structural
        } else {
            strategy
        };

        let mut chunks = match strategy {
            ChunkStrategy::Structural => {
                self.chunk_structural(&content, language.as_deref())
            }
            _ => self.chunk(&content, strategy)?,
        };

        // Add source path to all chunks
        for chunk in &mut chunks {
            chunk.source_path = Some(path.to_path_buf());
            chunk.metadata.language = language.clone();
        }

        Ok(chunks)
    }

    fn chunk_directory(&self, path: &Path, strategy: ChunkStrategy) -> Result<Vec<Chunk>> {
        let mut all_chunks = Vec::new();

        self.collect_directory_chunks(path, strategy, &mut all_chunks, 0, 4)?;

        Ok(all_chunks)
    }

    fn estimate_tokens(&self, content: &str) -> u32 {
        estimate_tokens(content)
    }
}

impl DefaultChunker {
    fn collect_directory_chunks(
        &self,
        dir: &Path,
        strategy: ChunkStrategy,
        chunks: &mut Vec<Chunk>,
        depth: usize,
        max_depth: usize,
    ) -> Result<()> {
        if depth > max_depth {
            return Ok(());
        }

        let entries = fs::read_dir(dir)
            .context("Failed to read directory")?;

        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // Skip hidden, node_modules, target, etc.
            if name.starts_with('.')
                || name == "node_modules"
                || name == "target"
                || name == "__pycache__"
                || name == "venv"
                || name == "dist"
                || name == "build"
            {
                continue;
            }

            if path.is_dir() {
                self.collect_directory_chunks(&path, strategy, chunks, depth + 1, max_depth)?;
            } else if self.should_include_file(&path) {
                match self.chunk_file(&path, strategy) {
                    Ok(file_chunks) => chunks.extend(file_chunks),
                    Err(e) => {
                        warn!("Failed to chunk {}: {}", path.display(), e);
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_fixed_chunking_with_overlap() {
        let chunker = DefaultChunker::new(100, 20);
        let content = "a".repeat(500);
        let chunks = chunker.chunk(&content, ChunkStrategy::FixedSize { size: 25, overlap: 5 }).unwrap();

        assert!(!chunks.is_empty());
        // Chunks should overlap
        for i in 1..chunks.len() {
            let prev_end = chunks[i-1].byte_range.unwrap().1;
            let curr_start = chunks[i].byte_range.unwrap().0;
            // Start should be before or at previous end (overlap)
            assert!(curr_start <= prev_end, "Chunks should overlap");
        }
    }

    #[test]
    fn test_structural_chunking_rust() {
        let chunker = DefaultChunker::default();
        let content = r#"
pub fn foo() {
    println!("foo");
}

pub fn bar() {
    println!("bar");
}

pub struct Baz {
    field: i32,
}
"#;
        let chunks = chunker.chunk_structural(content, Some("rust"));

        assert!(chunks.len() >= 2, "Should find at least 2 structural boundaries");
        assert!(chunks.iter().any(|c| c.metadata.structural_type == Some(StructuralType::Function)));
    }

    #[test]
    fn test_binary_file_detection() {
        assert!(DefaultChunker::is_binary(&[0, 1, 2, 3, 0]));
        assert!(!DefaultChunker::is_binary(b"hello world"));
    }

    #[test]
    fn test_token_estimation() {
        let chunker = DefaultChunker::default();
        let tokens = chunker.estimate_tokens("this is a test");
        assert!(tokens > 0);
        assert!(tokens < 10);
    }

    #[test]
    fn test_chunk_file() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.rs");
        fs::write(&file_path, "fn main() { println!(\"hello\"); }").unwrap();

        let chunker = DefaultChunker::default();
        let chunks = chunker.chunk_file(&file_path, ChunkStrategy::Structural).unwrap();

        assert!(!chunks.is_empty());
        assert!(chunks[0].source_path.is_some());
    }

    #[test]
    fn test_chunk_directory() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub fn foo() {}").unwrap();
        fs::write(src.join("main.rs"), "fn main() {}").unwrap();

        let chunker = DefaultChunker::default();
        let chunks = chunker.chunk_directory(temp.path(), ChunkStrategy::ByFile).unwrap();

        assert_eq!(chunks.len(), 2);
    }
}
