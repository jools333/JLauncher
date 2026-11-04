use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

pub mod icons;
pub mod linux;
pub mod windows;

#[derive(Debug, Clone)]
pub struct AppEntry {
    #[allow(dead_code)]
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
}

impl AppIndex {
    pub fn new() -> Self {
        let mut index = Self {
            apps: Vec::new(),
            matcher: SkimMatcherV2::default(),
        };
        index.refresh();
        index
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
            // Return top default apps when query is empty
            return self
                .apps
                .iter()
                .take(limit)
                .map(|entry| SearchMatch {
                    entry: entry.clone(),
                    score: 0,
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

            if let Some(score) = highest_score {
                matches.push(SearchMatch {
                    entry: app.clone(),
                    score,
                });
            }
        }

        matches.sort_by(|a, b| b.score.cmp(&a.score));
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
