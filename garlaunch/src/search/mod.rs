use crate::modes::Item;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Fuzzy matcher for filtering items
pub struct FuzzyMatcher {
    matcher: Matcher,
}

impl FuzzyMatcher {
    pub fn new() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    /// Filter items based on a query string
    pub fn filter(&mut self, items: &[Item], query: &str) -> Vec<Item> {
        if query.is_empty() {
            return items.to_vec();
        }

        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);

        let mut scored: Vec<(u32, &Item)> = items
            .iter()
            .filter_map(|item| {
                let mut buf = Vec::new();
                let haystack = Utf32Str::new(&item.name, &mut buf);
                let score = pattern.score(haystack, &mut self.matcher)?;

                // Also try matching against description
                if let Some(desc) = &item.description {
                    let mut desc_buf = Vec::new();
                    let desc_haystack = Utf32Str::new(desc, &mut desc_buf);
                    if let Some(desc_score) = pattern.score(desc_haystack, &mut self.matcher) {
                        // Use the better score
                        return Some((score.max(desc_score), item));
                    }
                }

                Some((score, item))
            })
            .collect();

        // Sort by score (descending)
        scored.sort_by(|a, b| b.0.cmp(&a.0));

        scored.into_iter().map(|(_, item)| item.clone()).collect()
    }
}

impl Default for FuzzyMatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_filter() {
        let items = vec![
            Item::new("firefox", "Firefox"),
            Item::new("chrome", "Google Chrome"),
            Item::new("code", "Visual Studio Code"),
            Item::new("term", "Terminal"),
        ];

        let mut matcher = FuzzyMatcher::new();

        let results = matcher.filter(&items, "fire");
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "firefox");

        let results = matcher.filter(&items, "code");
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "code");

        let results = matcher.filter(&items, "");
        assert_eq!(results.len(), items.len());
    }
}
