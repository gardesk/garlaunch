use crate::frecency::FrecencyStore;
use crate::modes::Item;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Fuzzy matcher for filtering items with optional frecency integration
pub struct FuzzyMatcher {
    matcher: Matcher,
    frecency: Option<FrecencyStore>,
}

impl FuzzyMatcher {
    pub fn new() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
            frecency: None,
        }
    }

    /// Attach a frecency store
    pub fn with_frecency(mut self, store: FrecencyStore) -> Self {
        self.frecency = Some(store);
        self
    }

    /// Get mutable reference to frecency store (for recording selections)
    pub fn frecency_mut(&mut self) -> Option<&mut FrecencyStore> {
        self.frecency.as_mut()
    }

    /// Filter items based on a query string
    pub fn filter(&mut self, items: &[Item], query: &str) -> Vec<Item> {
        if query.is_empty() {
            // No query: sort by frecency (or original order if no frecency)
            return self.sort_by_frecency(items);
        }

        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);

        let mut scored: Vec<(f64, &Item)> = items
            .iter()
            .filter_map(|item| {
                let mut buf = Vec::new();
                let haystack = Utf32Str::new(&item.name, &mut buf);
                let mut fuzzy_score = pattern.score(haystack, &mut self.matcher)? as f64;

                // Also try matching against description
                if let Some(desc) = &item.description {
                    let mut desc_buf = Vec::new();
                    let desc_haystack = Utf32Str::new(desc, &mut desc_buf);
                    if let Some(desc_score) = pattern.score(desc_haystack, &mut self.matcher) {
                        fuzzy_score = fuzzy_score.max(desc_score as f64);
                    }
                }

                // Add frecency boost (small factor so fuzzy dominates)
                let frecency_boost = self.frecency_score(&item.id) * 0.1;
                let combined = fuzzy_score + frecency_boost;

                Some((combined, item))
            })
            .collect();

        // Sort by combined score (descending)
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        scored.into_iter().map(|(_, item)| item.clone()).collect()
    }

    /// Sort items by frecency score (highest first), preserving order for unscored items
    fn sort_by_frecency(&self, items: &[Item]) -> Vec<Item> {
        let mut with_scores: Vec<(f64, usize, &Item)> = items
            .iter()
            .enumerate()
            .map(|(idx, item)| (self.frecency_score(&item.id), idx, item))
            .collect();

        // Sort by frecency (desc), then by original index (stable sort for unscored)
        with_scores.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.cmp(&b.1))
        });

        with_scores
            .into_iter()
            .map(|(_, _, item)| item.clone())
            .collect()
    }

    /// Get frecency score for an item
    fn frecency_score(&self, item_id: &str) -> f64 {
        self.frecency
            .as_ref()
            .map(|f| f.score(item_id))
            .unwrap_or(0.0)
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
