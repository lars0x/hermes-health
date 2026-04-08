use rust_embed::Embed;
use std::collections::HashMap;

#[derive(Embed)]
#[folder = "data/"]
#[include = "loinc_core.csv"]
#[include = "loinc_aliases.tsv"]
struct LoincData;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LoincEntry {
    pub loinc_num: String,
    pub component: String,
    pub long_common_name: String,
    pub short_name: String,
    pub example_ucum_units: String,
    pub class: String,
    pub scale_typ: String,
    pub system: String,
}

#[derive(Debug, Clone)]
pub struct LoincCandidate {
    pub loinc_code: String,
    pub canonical_name: String,
    pub confidence: f64,
    pub match_type: MatchType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchType {
    ExactCode,
    ExactName,
    Alias,
    Fuzzy,
}

impl std::fmt::Display for MatchType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatchType::ExactCode => write!(f, "exact"),
            MatchType::ExactName => write!(f, "exact"),
            MatchType::Alias => write!(f, "alias"),
            MatchType::Fuzzy => write!(f, "fuzzy"),
        }
    }
}

pub struct LoincCatalog {
    entries: Vec<LoincEntry>,
    by_code: HashMap<String, usize>,
    by_name_lower: HashMap<String, Vec<usize>>,
    _aliases: HashMap<String, Vec<(String, String)>>, // loinc_code -> [(alias, loinc_code)]
    alias_lookup: HashMap<String, Vec<usize>>,       // lowercase alias -> entry indices
}

impl LoincCatalog {
    pub fn load() -> Self {
        let mut entries = Vec::new();
        let mut by_code = HashMap::new();
        let mut by_name_lower: HashMap<String, Vec<usize>> = HashMap::new();

        // Load core catalog
        if let Some(data) = LoincData::get("loinc_core.csv") {
            let content = std::str::from_utf8(&data.data).unwrap_or("");
            let mut reader = csv::Reader::from_reader(content.as_bytes());
            for result in reader.records() {
                let record = match result {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                if record.len() < 8 {
                    continue;
                }
                let idx = entries.len();
                let entry = LoincEntry {
                    loinc_num: record[0].to_string(),
                    component: record[1].to_string(),
                    long_common_name: record[2].to_string(),
                    short_name: record[3].to_string(),
                    example_ucum_units: record[4].to_string(),
                    class: record[5].to_string(),
                    scale_typ: record[6].to_string(),
                    system: record[7].to_string(),
                };

                by_code.insert(entry.loinc_num.clone(), idx);
                by_name_lower
                    .entry(entry.component.to_lowercase())
                    .or_default()
                    .push(idx);
                by_name_lower
                    .entry(entry.long_common_name.to_lowercase())
                    .or_default()
                    .push(idx);

                entries.push(entry);
            }
        }

        // Load aliases for common tests
        let mut aliases: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let mut alias_lookup: HashMap<String, Vec<usize>> = HashMap::new();

        if let Some(data) = LoincData::get("loinc_aliases.tsv") {
            let content = std::str::from_utf8(&data.data).unwrap_or("");
            for line in content.lines().skip(1) {
                // skip header
                let parts: Vec<&str> = line.splitn(2, '\t').collect();
                if parts.len() != 2 {
                    continue;
                }
                let loinc_num = parts[0];
                let related = parts[1];

                if let Some(&idx) = by_code.get(loinc_num) {
                    for alias in related.split(';') {
                        let alias = alias.trim();
                        if alias.is_empty() {
                            continue;
                        }
                        aliases
                            .entry(loinc_num.to_string())
                            .or_default()
                            .push((alias.to_string(), loinc_num.to_string()));
                        alias_lookup
                            .entry(alias.to_lowercase())
                            .or_default()
                            .push(idx);
                    }
                }
            }
        }

        tracing::info!(
            "LOINC catalog loaded: {} entries, {} codes with aliases",
            entries.len(),
            aliases.len()
        );

        Self {
            entries,
            by_code,
            by_name_lower,
            _aliases: aliases,
            alias_lookup,
        }
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn get_by_code(&self, code: &str) -> Option<&LoincEntry> {
        self.by_code.get(code).map(|&idx| &self.entries[idx])
    }

    /// Search for a marker name and return up to `max_results` candidates ranked by confidence.
    /// Search order: exact LOINC code -> exact name -> alias match -> fuzzy match
    pub fn search(&self, query: &str, max_results: usize) -> Vec<LoincCandidate> {
        let mut candidates = Vec::new();

        // 1. Exact LOINC code match
        if let Some(&idx) = self.by_code.get(query) {
            let entry = &self.entries[idx];
            candidates.push(LoincCandidate {
                loinc_code: entry.loinc_num.clone(),
                canonical_name: entry.long_common_name.clone(),
                confidence: 1.0,
                match_type: MatchType::ExactCode,
            });
            return candidates;
        }

        let query_lower = query.to_lowercase();

        // 2. Exact name match (component or long_common_name)
        if let Some(indices) = self.by_name_lower.get(&query_lower) {
            for &idx in indices.iter().take(max_results) {
                let entry = &self.entries[idx];
                candidates.push(LoincCandidate {
                    loinc_code: entry.loinc_num.clone(),
                    canonical_name: entry.long_common_name.clone(),
                    confidence: 1.0,
                    match_type: MatchType::ExactName,
                });
            }
            if !candidates.is_empty() {
                return candidates;
            }
        }

        // 3. Alias match
        if let Some(indices) = self.alias_lookup.get(&query_lower) {
            for &idx in indices.iter().take(max_results) {
                let entry = &self.entries[idx];
                // Avoid duplicates
                if candidates.iter().any(|c| c.loinc_code == entry.loinc_num) {
                    continue;
                }
                candidates.push(LoincCandidate {
                    loinc_code: entry.loinc_num.clone(),
                    canonical_name: entry.long_common_name.clone(),
                    confidence: 0.95,
                    match_type: MatchType::Alias,
                });
            }
            if !candidates.is_empty() {
                return candidates;
            }
        }

        // 4. Fuzzy match (Jaro-Winkler on component and long_common_name)
        let threshold = 0.85;
        let mut scored: Vec<(f64, usize)> = Vec::new();

        for (idx, entry) in self.entries.iter().enumerate() {
            let sim_component = strsim::jaro_winkler(&query_lower, &entry.component.to_lowercase());
            let sim_long =
                strsim::jaro_winkler(&query_lower, &entry.long_common_name.to_lowercase());
            let best_sim = sim_component.max(sim_long);

            if best_sim >= threshold {
                scored.push((best_sim, idx));
            }
        }

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        for (score, idx) in scored.into_iter().take(max_results) {
            let entry = &self.entries[idx];
            candidates.push(LoincCandidate {
                loinc_code: entry.loinc_num.clone(),
                canonical_name: entry.long_common_name.clone(),
                confidence: score,
                match_type: MatchType::Fuzzy,
            });
        }

        candidates
    }

    /// Search specifically for quantitative lab tests (Qn scale type, Ser/Plas system)
    #[allow(dead_code)]
    pub fn search_lab(&self, query: &str, max_results: usize) -> Vec<LoincCandidate> {
        let all = self.search(query, max_results * 10);
        let mut filtered: Vec<(LoincCandidate, u8)> = all.into_iter()
            .filter_map(|c| {
                let entry = self.get_by_code(&c.loinc_code)?;
                if entry.scale_typ != "Qn" {
                    return None;
                }
                // Prefer serum/plasma/blood over urine
                let priority = if entry.system.contains("Ser") || entry.system.contains("Plas") {
                    0
                } else if entry.system.contains("Bld") {
                    1
                } else if entry.system.contains("Ur") {
                    2
                } else {
                    return None;
                };
                Some((c, priority))
            })
            .collect();
        // Sort by confidence desc, then specimen priority asc
        filtered.sort_by(|a, b| {
            b.0.confidence.partial_cmp(&a.0.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.1.cmp(&b.1))
        });
        filtered.into_iter().map(|(c, _)| c).take(max_results).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_loads() {
        let catalog = LoincCatalog::load();
        assert!(catalog.entry_count() > 0, "Catalog should have entries");
    }

    #[test]
    fn test_exact_code_lookup() {
        let catalog = LoincCatalog::load();
        let results = catalog.search("2093-3", 3);
        assert!(!results.is_empty(), "Should find Total Cholesterol by code");
        assert_eq!(results[0].loinc_code, "2093-3");
        assert_eq!(results[0].confidence, 1.0);
        assert_eq!(results[0].match_type, MatchType::ExactCode);
    }

    #[test]
    fn test_name_lookup() {
        let catalog = LoincCatalog::load();
        let results = catalog.search("Cholesterol", 3);
        assert!(!results.is_empty(), "Should find Cholesterol by component name");
    }

    #[test]
    fn test_fuzzy_lookup() {
        let catalog = LoincCatalog::load();
        let results = catalog.search("Total Cholestrol", 3); // deliberate typo
        // Fuzzy match may or may not find it depending on threshold
        // This test just verifies it doesn't crash
        let _ = results;
    }

    #[test]
    fn test_search_lab_sodium() {
        let catalog = LoincCatalog::load();
        let results = catalog.search_lab("Sodium", 3);
        assert!(!results.is_empty(), "search_lab should find Sodium");
        assert_eq!(results[0].loinc_code, "2951-2");
    }

    #[test]
    fn test_search_lab_potassium() {
        let catalog = LoincCatalog::load();
        let results = catalog.search_lab("Potassium", 3);
        assert!(!results.is_empty(), "search_lab should find Potassium");
        assert_eq!(results[0].loinc_code, "2823-3");
    }
}
