// In manual_sections.rs I can't easily add logging without importing log macros again if I missed them. 
// But I can try to add print statements for debugging since this is dev/test.
// Or actually adduse regex::Regex;
use regex::Regex;
use crate::config::ManualSectionConfig;
use std::collections::HashSet;
use std::collections::HashMap;

/// The regex pattern for manual section IDs.
const MANUAL_SECTION_ID: &str = "[a-zA-Z0-9_-]+";

#[derive(Clone)]
pub struct ManualSectionManager {
    config: ManualSectionConfig,
}

impl ManualSectionManager {
    pub fn new(config: ManualSectionConfig) -> Self {
        Self { config }
    }

    pub fn preserve_sections(&self, new_rendered: &str, prev_rendered: &str) -> String {
        // Build the regex pattern dynamically
        let manual_section_pattern = format!(
            r"{}:\s*({})(?:\s|$)(?s)(.*?){}",
            regex::escape(&self.config.start_marker),
            MANUAL_SECTION_ID,
            regex::escape(&self.config.end_marker)
        );
        
        // We also need a pattern to find starts without matching the full block necessarily, 
        // but for preservation we iterate over full blocks in new_rendered.
        
        let re = match Regex::new(&manual_section_pattern) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to compile regex for manual sections: {}", e);
                return new_rendered.to_string();
            }
        };

        let mut preserved = String::new();
        let mut last_end = 0;

        for cap in re.captures_iter(new_rendered) {
            let start = cap.get(0).unwrap().start();
            let end = cap.get(0).unwrap().end();
            let id = cap.get(1).unwrap().as_str();

            preserved.push_str(&new_rendered[last_end..start]);

            // Find the same section in the previous content
            // Note: This is inefficient for large files (re-scanning prev_rendered every time).
            // A better approach would be to extract all sections from prev_rendered once.
            // But keeping it simple for now to match logic structure.
            let prev_cap = re
                .captures_iter(prev_rendered)
                .find(|c| c.get(1).unwrap().as_str() == id);

            if let Some(prev_cap) = prev_cap {
                preserved.push_str(prev_cap.get(0).unwrap().as_str());
            } else {
                preserved.push_str(&new_rendered[start..end]);
            }

            last_end = end;
        }

        preserved.push_str(&new_rendered[last_end..]);
        preserved
    }

    /// Extract all section IDs from content
    pub fn extract_section_ids(&self, content: &str) -> Vec<String> {
        let pattern = format!(
            r"{}:\s*({})(?:\s|$)",
            regex::escape(&self.config.start_marker),
            MANUAL_SECTION_ID
        );
        let re = Regex::new(&pattern).unwrap(); // Should be safe if markers are safe
        re.captures_iter(content)
            .map(|cap| cap.get(1).unwrap().as_str().to_string())
            .collect()
    }

    /// Check for duplicate IDs
    pub fn check_duplicates(&self, content: &str, filename: &str) -> Result<(), String> {
        let ids = self.extract_section_ids(content);
        let mut seen = HashSet::new();
        let mut duplicates = Vec::new();

        for id in ids {
            if !seen.insert(id.clone()) {
                duplicates.push(id);
            }
        }

        if !duplicates.is_empty() {
            // Find line number of first duplicate
            // This is a bit rough, assuming we just need to return error
            return Err(format!("Duplicate manual section IDs in {:?}: {:?}", filename, duplicates));
        }
        Ok(())
    }

    /// Check for nested sections and unclosed sections
    pub fn check_structure(&self, content: &str, filename: &str) -> Result<(), String> {
        let start_marker = &self.config.start_marker;
        let end_marker = &self.config.end_marker;

        let start_count = content.matches(start_marker).count();
        let end_count = content.matches(end_marker).count();

        if start_count != end_count {
            return Err(format!(
                "Mismatched manual section markers in {:?}: {} starts, {} ends",
                filename, start_count, end_count
            ));
        }

        // Check for nesting
        // A simple way is to track depth. 
        // We find all start and end indices and sort them.
        let mut events = Vec::new();
        for (i, _) in content.match_indices(start_marker) {
            events.push((i, 1)); // 1 for start
        }
        for (i, _) in content.match_indices(end_marker) {
            events.push((i, -1)); // -1 for end
        }
        events.sort_by_key(|k| k.0);

        let mut depth = 0;
        for (_, change) in events {
            depth += change;
            if depth > 1 {
                return Err(format!("Nested manual sections detected in {:?}", filename));
            }
            if depth < 0 {
                return Err(format!("Manual section end before start in {:?}", filename));
            }
        }

        Ok(())
    }

    /// Validate sections across template, rendered, and previous content
    pub fn validate_sections(
        &self,
        template_path: &str,
        rendered: &str,
        prev_rendered: Option<&str>,
    ) -> Result<(), String> {
        // Check structure
        self.check_structure(rendered, template_path)?; // Using template path as name for rendered content associated with it
        self.check_duplicates(rendered, template_path)?;

        if let Some(prev) = prev_rendered {
             self.check_structure(prev, "existing file")?;
             self.check_duplicates(prev, "existing file")?;

             // Check for lost sections
             let curr_ids: HashSet<_> = self.extract_section_ids(rendered).into_iter().collect();
             let prev_ids = self.extract_section_ids(prev);

             for id in prev_ids {
                 if !curr_ids.contains(&id) {
                     return Err(format!(
                         "Manual section '{}' from existing file is missing in new template output for {:?}", 
                         id, template_path
                     ));
                 }
             }
        }

        Ok(())
    }

    /// Extract all section blocks (complete with markers) from content
    pub fn extract_blocks(&self, content: &str) -> HashMap<String, String> {
        let pattern = format!(
            r"({}:\s*({})(?:\s|$)(?s)(.*?){})",
            regex::escape(&self.config.start_marker),
            MANUAL_SECTION_ID,
            regex::escape(&self.config.end_marker)
        );
        let re = Regex::new(&pattern).unwrap();
        
        let mut blocks = HashMap::new();
        for cap in re.captures_iter(content) {
            let full_block = cap.get(1).unwrap().as_str().to_string();
            let id = cap.get(2).unwrap().as_str().to_string();
            blocks.insert(id, full_block);
        }
        blocks
    }

    /// Restore blocks into content
    pub fn restore_blocks(&self, content: &str, blocks: &HashMap<String, String>) -> String {
        let pattern = format!(
            r"{}:\s*({})(?:\s|$)(?s)(.*?){}",
            regex::escape(&self.config.start_marker),
            MANUAL_SECTION_ID,
            regex::escape(&self.config.end_marker)
        );
        let re = Regex::new(&pattern).unwrap();

        let mut result = String::new();
        let mut last_end = 0;

        for cap in re.captures_iter(content) {
            let start = cap.get(0).unwrap().start();
            let end = cap.get(0).unwrap().end();
            let id = cap.get(1).unwrap().as_str();

            result.push_str(&content[last_end..start]);

            if let Some(original_block) = blocks.get(id) {
                result.push_str(original_block);
            } else {
                result.push_str(&content[start..end]);
            }
            last_end = end;
        }
        result.push_str(&content[last_end..]);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duplicates() {
        let config = ManualSectionConfig::default();
        let manager = ManualSectionManager::new(config);
        let content = "
        MANUAL SECTION START: foo
        MANUAL SECTION END
        MANUAL SECTION START: foo
        MANUAL SECTION END
        ";
        assert!(manager.check_duplicates(content, "test").is_err());
    }

    #[test]
    fn test_nesting() {
        let config = ManualSectionConfig::default();
        let manager = ManualSectionManager::new(config);
        let content = "
        MANUAL SECTION START: foo
        MANUAL SECTION START: bar
        MANUAL SECTION END
        MANUAL SECTION END
        ";
        assert!(manager.check_structure(content, "test").is_err());
    }

    #[test]
    fn test_missing_section() {
        let config = ManualSectionConfig::default();
        let manager = ManualSectionManager::new(config);
        let old = "MANUAL SECTION START: keep_me\nMANUAL SECTION END";
        let new = "MANUAL SECTION START: other\nMANUAL SECTION END";
        assert!(manager.validate_sections("test", new, Some(old)).is_err());
    }
}
