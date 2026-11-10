use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

pub mod icons;
pub mod linux;
pub mod usage;
pub mod windows;

pub use usage::{calculate_usage_boost, AppUsage};

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub exec: String,
    pub icon: Option<String>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub entry: AppEntry,
    pub score: i64,
}

pub struct AppIndex {
    apps: Vec<AppEntry>,
    matcher: SkimMatcherV2,
    pub usage: AppUsage,
}

impl AppIndex {
    pub fn new() -> Self {
        let mut index = Self {
            apps: Vec::new(),
            matcher: SkimMatcherV2::default(),
            usage: AppUsage::new(),
        };
        index.refresh();
        index
    }

    #[allow(dead_code)]
    pub fn with_usage(usage: AppUsage) -> Self {
        let mut index = Self {
            apps: Vec::new(),
            matcher: SkimMatcherV2::default(),
            usage,
        };
        index.refresh();
        index
    }

    #[cfg(test)]
    pub fn with_apps_and_usage(apps: Vec<AppEntry>, usage: AppUsage) -> Self {
        Self {
            apps,
            matcher: SkimMatcherV2::default(),
            usage,
        }
    }

    pub fn record_launch(&mut self, entry: &AppEntry) -> u64 {
        self.usage.record_launch(entry)
    }

    pub fn get_launch_count(&self, entry: &AppEntry) -> u64 {
        self.usage.get_count_for_app(entry)
    }

    pub fn clear_usage(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.usage.clear()
    }

    pub fn refresh(&mut self) {
        let mut loaded = Vec::new();

        #[cfg(target_os = "windows")]
        {
            loaded.extend(windows::scan_windows_apps());
        }

        #[cfg(not(target_os = "windows"))]
        {
            loaded.extend(linux::scan_linux_apps());
        }

        // Deduplicate apps by name or exec
        let mut unique: Vec<AppEntry> = Vec::new();
        for app in loaded {
            let exists = unique
                .iter()
                .any(|u| u.name.eq_ignore_ascii_case(&app.name));
            if !exists {
                unique.push(app);
            }
        }

        unique.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.apps = unique;
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchMatch> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            // Initial view: Return apps sorted by launch count descending,
            // with ties sorted alphabetically.
            let mut indices: Vec<usize> = (0..self.apps.len()).collect();
            indices.sort_by(|&i, &j| {
                let a = &self.apps[i];
                let b = &self.apps[j];
                let count_a = self.usage.get_count_for_app(a);
                let count_b = self.usage.get_count_for_app(b);
                count_b
                    .cmp(&count_a)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });

            return indices
                .into_iter()
                .take(limit)
                .map(|idx| {
                    let entry = self.apps[idx].clone();
                    let count = self.usage.get_count_for_app(&entry);
                    SearchMatch {
                        entry,
                        score: count as i64,
                    }
                })
                .collect();
        }

        let mut matches = Vec::new();

        for app in &self.apps {
            let mut highest_score = self.matcher.fuzzy_match(&app.name, trimmed);

            // Also check keywords / exec / description if name didn't match well
            if let Some(desc_score) = self.matcher.fuzzy_match(&app.description, trimmed) {
                highest_score =
                    Some(highest_score.map_or(desc_score / 2, |s| s.max(desc_score / 2)));
            }

            for kw in &app.keywords {
                if let Some(kw_score) = self.matcher.fuzzy_match(kw, trimmed) {
                    highest_score =
                        Some(highest_score.map_or(kw_score / 2, |s| s.max(kw_score / 2)));
                }
            }

            if let Some(base_score) = highest_score {
                let count = self.usage.get_count_for_app(app);
                let boost = calculate_usage_boost(count);
                let score = base_score.saturating_add(boost);

                matches.push(SearchMatch {
                    entry: app.clone(),
                    score,
                });
            }
        }

        matches.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| {
                    let count_a = self.usage.get_count_for_app(&a.entry);
                    let count_b = self.usage.get_count_for_app(&b.entry);
                    count_b.cmp(&count_a)
                })
                .then_with(|| a.entry.name.to_lowercase().cmp(&b.entry.name.to_lowercase()))
        });
        matches.truncate(limit);
        matches
    }

    pub fn launch(entry: &AppEntry) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            windows::launch_app(entry)
        }

        #[cfg(not(target_os = "windows"))]
        {
            linux::launch_app(entry)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_app(id: &str, name: &str) -> AppEntry {
        AppEntry {
            id: id.to_string(),
            name: name.to_string(),
            description: String::new(),
            exec: format!("/usr/bin/{}", id),
            icon: None,
            keywords: vec![],
        }
    }

    #[test]
    fn test_empty_query_sorts_most_frequently_used_first() {
        let app_a = create_test_app("app-a", "Alpha");
        let app_b = create_test_app("app-b", "Beta");
        let app_c = create_test_app("app-c", "Charlie");
        let app_d = create_test_app("app-d", "Delta");

        let apps = vec![app_a.clone(), app_b.clone(), app_c.clone(), app_d.clone()];
        let mut usage = AppUsage::default();

        // Launch Beta 5 times, Delta 10 times, Alpha 0 times, Charlie 1 time
        for _ in 0..5 {
            usage.record_launch(&app_b);
        }
        for _ in 0..10 {
            usage.record_launch(&app_d);
        }
        usage.record_launch(&app_c);

        let index = AppIndex::with_apps_and_usage(apps, usage);
        let results = index.search("", 10);

        assert_eq!(results.len(), 4);
        assert_eq!(results[0].entry.name, "Delta"); // count 10
        assert_eq!(results[1].entry.name, "Beta"); // count 5
        assert_eq!(results[2].entry.name, "Charlie"); // count 1
        assert_eq!(results[3].entry.name, "Alpha"); // count 0
    }

    #[test]
    fn test_search_considers_launch_count() {
        let app_chrome = create_test_app("google-chrome", "Google Chrome");
        let app_calc = create_test_app("calculator", "Calculator");

        let apps = vec![app_chrome.clone(), app_calc.clone()];
        let mut usage = AppUsage::default();

        // Before any launch: query "c"
        let index_initial = AppIndex::with_apps_and_usage(apps.clone(), usage.clone());
        let results_initial = index_initial.search("c", 10);
        assert_eq!(results_initial.len(), 2);
        // "Calculator" starts with 'C', while "Google Chrome" has 'C' as 8th char
        assert_eq!(results_initial[0].entry.name, "Calculator");

        // Now user launches Google Chrome 30 times
        for _ in 0..30 {
            usage.record_launch(&app_chrome);
        }

        let index_boosted = AppIndex::with_apps_and_usage(apps, usage);
        let results_boosted = index_boosted.search("c", 10);
        // With usage boost, the frequently launched Chrome ranks first
        assert_eq!(results_boosted[0].entry.name, "Google Chrome");

        // But when user specifically types "calc", Calculator is an exact prefix match and wins
        let results_calc = index_boosted.search("calc", 10);
        assert_eq!(results_calc[0].entry.name, "Calculator");
    }
}
