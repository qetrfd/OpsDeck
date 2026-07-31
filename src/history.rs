use crate::ProjectStatus;
use crate::health::HealthCheck;
use crate::intelligence::Diagnosis;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_REVIEWS_PER_PROJECT: usize = 200;
const MAX_FEEDBACK_ENTRIES: usize = 2_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub project_name: String,
    pub checked_at: u64,
    pub score: u8,
    pub risk: String,
    pub health_state: String,
    pub status_code: Option<u16>,
    pub latency_ms: Option<u64>,
    pub branch: String,
    pub changes_total: usize,
    pub changes_staged: usize,
    pub changes_unstaged: usize,
    pub changes_untracked: usize,
    pub commits_ahead: usize,
    pub commits_behind: usize,
    pub finding_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackRecord {
    pub project_name: String,
    pub rule_code: String,
    pub useful: bool,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistoryStore {
    #[serde(default)]
    pub reviews: Vec<ReviewRecord>,

    #[serde(default)]
    pub feedback: Vec<FeedbackRecord>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FeedbackSummary {
    pub useful: usize,
    pub not_useful: usize,
}

pub fn history_path() -> Result<PathBuf, String> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "No se pudo localizar la carpeta del usuario".to_string())?;

    let directory = home.join(".opsdeck");

    fs::create_dir_all(&directory)
        .map_err(|error| format!("No se pudo crear {}: {error}", directory.display()))?;

    Ok(directory.join("history.json"))
}

pub fn load_history() -> Result<HistoryStore, String> {
    let path = history_path()?;

    if !path.exists() {
        return Ok(HistoryStore::default());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("No se pudo leer {}: {error}", path.display()))?;

    if content.trim().is_empty() {
        return Ok(HistoryStore::default());
    }

    serde_json::from_str(&content)
        .map_err(|error| format!("El historial {} no es válido: {error}", path.display()))
}

pub fn save_history(history: &HistoryStore) -> Result<(), String> {
    let path = history_path()?;

    let content = serde_json::to_string_pretty(history)
        .map_err(|error| format!("No se pudo convertir el historial a JSON: {error}"))?;

    fs::write(&path, content)
        .map_err(|error| format!("No se pudo guardar {}: {error}", path.display()))
}

pub fn record_review(
    project_name: &str,
    status: &ProjectStatus,
    health: &HealthCheck,
    diagnosis: &Diagnosis,
) -> Result<ReviewRecord, String> {
    let project_name = project_name.trim();

    if project_name.is_empty() {
        return Err("El nombre del proyecto no puede estar vacío".to_string());
    }

    let record = ReviewRecord {
        project_name: project_name.to_string(),
        checked_at: unix_timestamp(),
        score: diagnosis.score,
        risk: diagnosis.risk.to_string(),
        health_state: health.state.to_string(),
        status_code: health.status_code,

        latency_ms: health
            .latency_ms
            .map(|value| u64::try_from(value).unwrap_or(u64::MAX)),

        branch: status.branch.clone(),
        changes_total: status.changes.total,
        changes_staged: status.changes.staged,
        changes_unstaged: status.changes.unstaged,
        changes_untracked: status.changes.untracked,
        commits_ahead: status.sync.ahead,
        commits_behind: status.sync.behind,

        finding_codes: diagnosis
            .findings
            .iter()
            .map(|finding| finding.code.clone())
            .collect(),
    };

    let mut history = load_history()?;

    history.reviews.push(record.clone());

    trim_reviews(&mut history.reviews);

    save_history(&history)?;

    Ok(record)
}

pub fn record_feedback(
    project_name: &str,
    rule_code: &str,
    useful: bool,
) -> Result<FeedbackRecord, String> {
    let project_name = project_name.trim();
    let rule_code = rule_code.trim();

    if project_name.is_empty() {
        return Err("El nombre del proyecto no puede estar vacío".to_string());
    }

    if rule_code.is_empty() {
        return Err("El código de la regla no puede estar vacío".to_string());
    }

    let record = FeedbackRecord {
        project_name: project_name.to_string(),
        rule_code: rule_code.to_uppercase(),
        useful,
        created_at: unix_timestamp(),
    };

    let mut history = load_history()?;

    history.feedback.push(record.clone());

    if history.feedback.len() > MAX_FEEDBACK_ENTRIES {
        let excess = history.feedback.len() - MAX_FEEDBACK_ENTRIES;

        history.feedback.drain(0..excess);
    }

    save_history(&history)?;

    Ok(record)
}

pub fn recent_reviews(project_name: &str, limit: usize) -> Result<Vec<ReviewRecord>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let history = load_history()?;

    let reviews = history
        .reviews
        .iter()
        .rev()
        .filter(|record| record.project_name.eq_ignore_ascii_case(project_name))
        .take(limit)
        .cloned()
        .collect();

    Ok(reviews)
}

pub fn feedback_summary(project_name: &str, rule_code: &str) -> Result<FeedbackSummary, String> {
    let history = load_history()?;

    Ok(feedback_summary_from_store(
        &history,
        project_name,
        rule_code,
    ))
}

pub fn feedback_for_project(
    project_name: &str,
) -> Result<HashMap<String, FeedbackSummary>, String> {
    let history = load_history()?;

    let mut summaries = HashMap::<String, FeedbackSummary>::new();

    for feedback in &history.feedback {
        if !feedback.project_name.eq_ignore_ascii_case(project_name) {
            continue;
        }

        let code = feedback.rule_code.to_uppercase();

        let summary = summaries.entry(code).or_default();

        if feedback.useful {
            summary.useful += 1;
        } else {
            summary.not_useful += 1;
        }
    }

    Ok(summaries)
}

pub fn clear_project_history(project_name: &str) -> Result<usize, String> {
    let mut history = load_history()?;

    let previous_count = history.reviews.len();

    history
        .reviews
        .retain(|record| !record.project_name.eq_ignore_ascii_case(project_name));

    let removed = previous_count - history.reviews.len();

    save_history(&history)?;

    Ok(removed)
}

pub fn clear_project_feedback(project_name: &str) -> Result<usize, String> {
    let mut history = load_history()?;

    let previous_count = history.feedback.len();

    history
        .feedback
        .retain(|record| !record.project_name.eq_ignore_ascii_case(project_name));

    let removed = previous_count - history.feedback.len();

    save_history(&history)?;

    Ok(removed)
}

fn trim_reviews(reviews: &mut Vec<ReviewRecord>) {
    let mut project_names = reviews
        .iter()
        .map(|record| record.project_name.to_lowercase())
        .collect::<Vec<_>>();

    project_names.sort();
    project_names.dedup();

    for project_name in project_names {
        let project_count = reviews
            .iter()
            .filter(|record| record.project_name.to_lowercase() == project_name)
            .count();

        if project_count <= MAX_REVIEWS_PER_PROJECT {
            continue;
        }

        let mut to_remove = project_count - MAX_REVIEWS_PER_PROJECT;

        reviews.retain(|record| {
            if record.project_name.to_lowercase() == project_name && to_remove > 0 {
                to_remove -= 1;
                false
            } else {
                true
            }
        });
    }
}

fn feedback_summary_from_store(
    history: &HistoryStore,
    project_name: &str,
    rule_code: &str,
) -> FeedbackSummary {
    let mut summary = FeedbackSummary::default();

    for feedback in &history.feedback {
        let same_project = feedback.project_name.eq_ignore_ascii_case(project_name);

        let same_rule = feedback.rule_code.eq_ignore_ascii_case(rule_code);

        if same_project && same_rule {
            if feedback.useful {
                summary.useful += 1;
            } else {
                summary.not_useful += 1;
            }
        }
    }

    summary
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review(project_name: &str, checked_at: u64) -> ReviewRecord {
        ReviewRecord {
            project_name: project_name.to_string(),
            checked_at,
            score: 100,
            risk: "Saludable".to_string(),
            health_state: "Saludable".to_string(),
            status_code: Some(200),
            latency_ms: Some(100),
            branch: "main".to_string(),
            changes_total: 0,
            changes_staged: 0,
            changes_unstaged: 0,
            changes_untracked: 0,
            commits_ahead: 0,
            commits_behind: 0,
            finding_codes: Vec::new(),
        }
    }

    #[test]
    fn review_limit_is_applied_per_project() {
        let mut reviews = Vec::new();

        for index in 0..250 {
            reviews.push(review("Demo", index));
        }

        reviews.push(review("Otro", 1));

        trim_reviews(&mut reviews);

        let demo_count = reviews
            .iter()
            .filter(|record| record.project_name == "Demo")
            .count();

        let other_count = reviews
            .iter()
            .filter(|record| record.project_name == "Otro")
            .count();

        assert_eq!(demo_count, MAX_REVIEWS_PER_PROJECT);

        assert_eq!(other_count, 1);
    }

    #[test]
    fn feedback_summary_counts_both_answers() {
        let history = HistoryStore {
            reviews: Vec::new(),

            feedback: vec![
                FeedbackRecord {
                    project_name: "Demo".to_string(),
                    rule_code: "RULE_ONE".to_string(),
                    useful: true,
                    created_at: 1,
                },
                FeedbackRecord {
                    project_name: "Demo".to_string(),
                    rule_code: "RULE_ONE".to_string(),
                    useful: true,
                    created_at: 2,
                },
                FeedbackRecord {
                    project_name: "Demo".to_string(),
                    rule_code: "RULE_ONE".to_string(),
                    useful: false,
                    created_at: 3,
                },
            ],
        };

        let summary = feedback_summary_from_store(&history, "demo", "rule_one");

        assert_eq!(summary.useful, 2);
        assert_eq!(summary.not_useful, 1);
    }

    #[test]
    fn feedback_for_project_groups_rules() {
        let history = HistoryStore {
            reviews: Vec::new(),

            feedback: vec![
                FeedbackRecord {
                    project_name: "Demo".to_string(),
                    rule_code: "RULE_ONE".to_string(),
                    useful: true,
                    created_at: 1,
                },
                FeedbackRecord {
                    project_name: "Demo".to_string(),
                    rule_code: "rule_one".to_string(),
                    useful: false,
                    created_at: 2,
                },
                FeedbackRecord {
                    project_name: "Otro".to_string(),
                    rule_code: "RULE_ONE".to_string(),
                    useful: true,
                    created_at: 3,
                },
            ],
        };

        let mut summaries = HashMap::<String, FeedbackSummary>::new();

        for feedback in &history.feedback {
            if !feedback.project_name.eq_ignore_ascii_case("Demo") {
                continue;
            }

            let summary = summaries
                .entry(feedback.rule_code.to_uppercase())
                .or_default();

            if feedback.useful {
                summary.useful += 1;
            } else {
                summary.not_useful += 1;
            }
        }

        let summary = summaries
            .get("RULE_ONE")
            .copied()
            .expect("Debe existir RULE_ONE");

        assert_eq!(summary.useful, 1);
        assert_eq!(summary.not_useful, 1);
    }
}
