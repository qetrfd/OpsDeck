use crate::ProjectStatus;
use crate::anomaly::AnomalyReport;
use crate::checklist::{CheckState, DeployChecklist};
use crate::health::{HealthCheck, HealthState};
use crate::intelligence::Diagnosis;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateDecision {
    Approved,
    ApprovedWithWarnings,
    Blocked,
}

impl fmt::Display for GateDecision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Approved => "Aprobado",
            Self::ApprovedWithWarnings => "Aprobado con advertencias",
            Self::Blocked => "Bloqueado",
        };

        write!(formatter, "{label}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PolicyPreset {
    Development,

    #[default]
    Balanced,

    Production,
}

impl PolicyPreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::Development => "Desarrollo",
            Self::Balanced => "Equilibrada",
            Self::Production => "Producción",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Development => "Permite trabajar con cambios locales y advertencias.",

            Self::Balanced => "Bloquea problemas importantes sin exigir condiciones de producción.",

            Self::Production => "Aplica controles estrictos antes de permitir un deploy.",
        }
    }
}

impl fmt::Display for PolicyPreset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Development => "development",
            Self::Balanced => "balanced",
            Self::Production => "production",
        };

        write!(formatter, "{value}")
    }
}

impl FromStr for PolicyPreset {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "development" | "develop" | "dev" | "desarrollo" => Ok(Self::Development),

            "balanced" | "balance" | "default" | "equilibrada" | "equilibrado" => {
                Ok(Self::Balanced)
            }

            "production" | "prod" | "produccion" | "producción" => Ok(Self::Production),

            other => Err(format!(
                "Política desconocida: {other}. Usa development, balanced o production."
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeployPolicy {
    pub preset: PolicyPreset,
    pub strict_warnings: bool,
    pub minimum_score: u8,
    pub require_health: bool,
    pub require_clean_tree: bool,
    pub max_latency_ms: Option<u64>,
    pub allow_commits_ahead: bool,
}

impl Default for DeployPolicy {
    fn default() -> Self {
        Self::from_preset(PolicyPreset::Balanced)
    }
}

impl DeployPolicy {
    pub fn from_preset(preset: PolicyPreset) -> Self {
        match preset {
            PolicyPreset::Development => Self {
                preset,
                strict_warnings: false,
                minimum_score: 50,
                require_health: false,
                require_clean_tree: false,
                max_latency_ms: None,
                allow_commits_ahead: true,
            },

            PolicyPreset::Balanced => Self {
                preset,
                strict_warnings: false,
                minimum_score: 75,
                require_health: false,
                require_clean_tree: true,
                max_latency_ms: Some(2_500),
                allow_commits_ahead: true,
            },

            PolicyPreset::Production => Self {
                preset,
                strict_warnings: true,
                minimum_score: 90,
                require_health: true,
                require_clean_tree: true,
                max_latency_ms: Some(1_000),
                allow_commits_ahead: false,
            },
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.minimum_score > 100 {
            return Err("La puntuación mínima no puede ser mayor a 100.".to_string());
        }

        if matches!(self.max_latency_ms, Some(0)) {
            return Err("La latencia máxima debe ser mayor a cero.".to_string());
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectPolicy {
    project_name: String,
    policy: DeployPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PolicyStore {
    #[serde(default)]
    projects: Vec<ProjectPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateManifest {
    pub project_name: String,
    pub generated_at: u64,
    pub decision: String,
    pub ready: bool,

    pub policy_preset: String,
    pub policy_label: String,
    pub strict_warnings: bool,
    pub minimum_score: u8,
    pub require_health: bool,
    pub require_clean_tree: bool,
    pub max_latency_ms: Option<u64>,
    pub allow_commits_ahead: bool,

    pub score: u8,
    pub risk: String,
    pub branch: String,
    pub last_commit: String,
    pub health_state: String,
    pub status_code: Option<u16>,
    pub latency_ms: Option<u64>,

    pub checklist_passed: usize,
    pub checklist_warnings: usize,
    pub checklist_failed: usize,
    pub anomaly_count: usize,

    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DeployGate {
    pub decision: GateDecision,
    pub ready: bool,
    pub summary: String,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub policy: DeployPolicy,
    pub manifest: GateManifest,
}

impl DeployGate {
    pub fn unavailable(
        project_name: impl Into<String>,
        reason: impl Into<String>,
        policy: &DeployPolicy,
    ) -> Self {
        let project_name = project_name.into();

        let reason = reason.into();

        let manifest = GateManifest {
            project_name,
            generated_at: unix_timestamp(),
            decision: GateDecision::Blocked.to_string(),
            ready: false,

            policy_preset: policy.preset.to_string(),

            policy_label: policy.preset.label().to_string(),

            strict_warnings: policy.strict_warnings,

            minimum_score: policy.minimum_score,

            require_health: policy.require_health,

            require_clean_tree: policy.require_clean_tree,

            max_latency_ms: policy.max_latency_ms,

            allow_commits_ahead: policy.allow_commits_ahead,

            score: 0,
            risk: "No disponible".to_string(),
            branch: "No disponible".to_string(),
            last_commit: "No disponible".to_string(),
            health_state: "No disponible".to_string(),
            status_code: None,
            latency_ms: None,
            checklist_passed: 0,
            checklist_warnings: 0,
            checklist_failed: 1,
            anomaly_count: 0,
            blockers: vec![reason.clone()],
            warnings: Vec::new(),
        };

        Self {
            decision: GateDecision::Blocked,
            ready: false,

            summary: format!(
                "Deploy bloqueado porque OpsDeck no pudo completar la evaluación: {reason}"
            ),

            blockers: vec![reason],
            warnings: Vec::new(),
            policy: policy.clone(),
            manifest,
        }
    }
}

pub fn policy_path() -> Result<PathBuf, String> {
    crate::paths::data_file("policies.json")
}

pub fn load_policy(project_name: &str) -> Result<DeployPolicy, String> {
    let project_name = project_name.trim();

    if project_name.is_empty() {
        return Err("El nombre del proyecto no puede estar vacío.".to_string());
    }

    let store = load_policy_store()?;

    let policy = store
        .projects
        .iter()
        .find(|entry| entry.project_name.eq_ignore_ascii_case(project_name))
        .map(|entry| entry.policy.clone())
        .unwrap_or_default();

    policy.validate()?;

    Ok(policy)
}

pub fn save_policy(project_name: &str, policy: &DeployPolicy) -> Result<PathBuf, String> {
    let project_name = project_name.trim();

    if project_name.is_empty() {
        return Err("El nombre del proyecto no puede estar vacío.".to_string());
    }

    policy.validate()?;

    let mut store = load_policy_store()?;

    match store
        .projects
        .iter_mut()
        .find(|entry| entry.project_name.eq_ignore_ascii_case(project_name))
    {
        Some(entry) => {
            entry.project_name = project_name.to_string();

            entry.policy = policy.clone();
        }

        None => {
            store.projects.push(ProjectPolicy {
                project_name: project_name.to_string(),

                policy: policy.clone(),
            });
        }
    }

    store.projects.sort_by(|left, right| {
        left.project_name
            .to_lowercase()
            .cmp(&right.project_name.to_lowercase())
    });

    save_policy_store(&store)
}

pub fn reset_policy(project_name: &str) -> Result<bool, String> {
    let project_name = project_name.trim();

    if project_name.is_empty() {
        return Err("El nombre del proyecto no puede estar vacío.".to_string());
    }

    let mut store = load_policy_store()?;

    let previous_count = store.projects.len();

    store
        .projects
        .retain(|entry| !entry.project_name.eq_ignore_ascii_case(project_name));

    let removed = previous_count != store.projects.len();

    if removed {
        save_policy_store(&store)?;
    }

    Ok(removed)
}

pub fn evaluate_deploy_gate(
    project_name: &str,
    status: &ProjectStatus,
    health: &HealthCheck,
    diagnosis: &Diagnosis,
    anomaly_report: &AnomalyReport,
    checklist: &DeployChecklist,
    policy: &DeployPolicy,
) -> DeployGate {
    let mut blockers = Vec::<String>::new();

    let mut warnings = Vec::<String>::new();

    for item in &checklist.items {
        match item.state {
            CheckState::Passed => {}

            CheckState::Warning => {
                push_unique(&mut warnings, format!("{}: {}", item.code, item.title));
            }

            CheckState::Failed => {
                if item.code == "WORKING_TREE_CLEAN" && !policy.require_clean_tree {
                    push_unique(
                        &mut warnings,
                        format!("POLICY_OVERRIDE_DIRTY_TREE: {}", item.title),
                    );
                } else {
                    push_unique(&mut blockers, format!("{}: {}", item.code, item.title));
                }
            }
        }
    }

    if diagnosis.score < policy.minimum_score {
        push_unique(
            &mut blockers,
            format!(
                "POLICY_MINIMUM_SCORE: la puntuación {}/100 es menor al mínimo requerido de {}/100",
                diagnosis.score, policy.minimum_score
            ),
        );
    }

    if policy.require_health && matches!(health.state, HealthState::NotConfigured) {
        push_unique(
            &mut blockers,
            "POLICY_HEALTH_REQUIRED: la política requiere un endpoint de health".to_string(),
        );
    }

    if !policy.allow_commits_ahead && status.sync.ahead > 0 {
        push_unique(
            &mut blockers,
            format!(
                "POLICY_COMMITS_AHEAD: existen {} commits pendientes de subir",
                status.sync.ahead
            ),
        );
    }

    if let (Some(maximum_latency), Some(current_latency)) =
        (policy.max_latency_ms, health.latency_ms)
    {
        let current_latency = u64::try_from(current_latency).unwrap_or(u64::MAX);

        if current_latency > maximum_latency {
            push_unique(
                &mut blockers,
                format!(
                    "POLICY_MAX_LATENCY: la latencia de {current_latency} ms supera el máximo de {maximum_latency} ms"
                ),
            );
        }
    }

    if policy.strict_warnings && !warnings.is_empty() {
        push_unique(
            &mut blockers,
            format!(
                "POLICY_STRICT_WARNINGS: la política {} bloquea las {} advertencia(s) activas",
                policy.preset,
                warnings.len()
            ),
        );
    }

    let decision = if !blockers.is_empty() {
        GateDecision::Blocked
    } else if !warnings.is_empty() {
        GateDecision::ApprovedWithWarnings
    } else {
        GateDecision::Approved
    };

    let ready = decision != GateDecision::Blocked;

    let summary = match decision {
        GateDecision::Approved => {
            format!(
                "Deploy aprobado con la política {}. Los {} requisitos fueron superados.",
                policy.preset, checklist.passed
            )
        }

        GateDecision::ApprovedWithWarnings => {
            format!(
                "Deploy aprobado con la política {} y {} advertencia(s).",
                policy.preset,
                warnings.len()
            )
        }

        GateDecision::Blocked => {
            format!(
                "Deploy bloqueado por la política {}: {} bloqueo(s) y {} advertencia(s).",
                policy.preset,
                blockers.len(),
                warnings.len()
            )
        }
    };

    let manifest = GateManifest {
        project_name: project_name.to_string(),

        generated_at: unix_timestamp(),

        decision: decision.to_string(),

        ready,

        policy_preset: policy.preset.to_string(),

        policy_label: policy.preset.label().to_string(),

        strict_warnings: policy.strict_warnings,

        minimum_score: policy.minimum_score,

        require_health: policy.require_health,

        require_clean_tree: policy.require_clean_tree,

        max_latency_ms: policy.max_latency_ms,

        allow_commits_ahead: policy.allow_commits_ahead,

        score: diagnosis.score,

        risk: diagnosis.risk.to_string(),

        branch: status.branch.clone(),

        last_commit: status.last_commit.clone(),

        health_state: health.state.to_string(),

        status_code: health.status_code,

        latency_ms: health
            .latency_ms
            .map(|value| u64::try_from(value).unwrap_or(u64::MAX)),

        checklist_passed: checklist.passed,

        checklist_warnings: checklist.warnings,

        checklist_failed: checklist.failed,

        anomaly_count: anomaly_report.anomalies.len(),

        blockers: blockers.clone(),

        warnings: warnings.clone(),
    };

    DeployGate {
        decision,
        ready,
        summary,
        blockers,
        warnings,
        policy: policy.clone(),
        manifest,
    }
}

pub fn export_gate_manifest(gate: &DeployGate, output: Option<&Path>) -> Result<PathBuf, String> {
    let path = match output {
        Some(path) => path.to_path_buf(),

        None => default_gate_path(&gate.manifest.project_name)?,
    };

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("No se pudo crear {}: {error}", parent.display()))?;
    }

    let content = serde_json::to_string_pretty(&gate.manifest)
        .map_err(|error| format!("No se pudo generar el manifiesto JSON: {error}"))?;

    fs::write(&path, content)
        .map_err(|error| format!("No se pudo guardar {}: {error}", path.display()))?;

    Ok(path)
}

pub fn suggested_gate_filename(project_name: &str) -> String {
    format!("{}-deploy-gate.json", slugify(project_name))
}

pub fn default_gate_path(project_name: &str) -> Result<PathBuf, String> {
    let directory = crate::paths::data_subdir("gates")?;

    Ok(directory.join(format!(
        "{}-deploy-gate-{}.json",
        slugify(project_name),
        unix_timestamp()
    )))
}

fn load_policy_store() -> Result<PolicyStore, String> {
    let path = policy_path()?;

    if !path.exists() {
        return Ok(PolicyStore::default());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("No se pudo leer {}: {error}", path.display()))?;

    if content.trim().is_empty() {
        return Ok(PolicyStore::default());
    }

    serde_json::from_str(&content).map_err(|error| {
        format!(
            "El archivo de políticas {} no es válido: {error}",
            path.display()
        )
    })
}

fn save_policy_store(store: &PolicyStore) -> Result<PathBuf, String> {
    let path = policy_path()?;

    let content = serde_json::to_string_pretty(store)
        .map_err(|error| format!("No se pudieron convertir las políticas a JSON: {error}"))?;

    let temporary_path = path.with_extension("json.tmp");

    fs::write(&temporary_path, content)
        .map_err(|error| format!("No se pudo guardar {}: {error}", temporary_path.display()))?;

    fs::rename(&temporary_path, &path)
        .map_err(|error| format!("No se pudo actualizar {}: {error}", path.display()))?;

    Ok(path)
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn slugify(value: &str) -> String {
    let mut result = String::new();

    let mut separator = false;

    for character in value.trim().to_lowercase().chars() {
        if character.is_alphanumeric() {
            result.push(character);
            separator = false;
        } else if !separator && !result.is_empty() {
            result.push('-');
            separator = true;
        }
    }

    while result.ends_with('-') {
        result.pop();
    }

    if result.is_empty() {
        "project".to_string()
    } else {
        result
    }
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
    use crate::anomaly::AnomalyReport;
    use crate::checklist::{ChecklistItem, DeployChecklist};
    use crate::health::{HealthCheck, HealthState};
    use crate::intelligence::{Diagnosis, RiskLevel};
    use crate::{ChangeSummary, ProjectStatus, SyncStatus};

    fn status() -> ProjectStatus {
        ProjectStatus {
            name: "Demo".to_string(),

            path: PathBuf::from("/tmp/demo"),

            registered: true,

            health_url: Some("https://example.com/health".to_string()),

            branch: "main".to_string(),

            changes: ChangeSummary::default(),

            last_commit: "abc123 | demo".to_string(),

            remote: "https://github.com/demo/demo.git".to_string(),

            sync: SyncStatus {
                upstream: Some("origin/main".to_string()),
                ahead: 0,
                behind: 0,
            },

            raw_status: String::new(),
        }
    }

    fn health() -> HealthCheck {
        HealthCheck {
            url: Some("https://example.com/health".to_string()),

            state: HealthState::Healthy,

            status_code: Some(200),

            latency_ms: Some(100),

            content_type: Some("application/json".to_string()),

            json_valid: Some(true),

            body_preview: None,

            error: None,
        }
    }

    fn diagnosis(score: u8) -> Diagnosis {
        Diagnosis {
            score,

            risk: RiskLevel::Healthy,

            summary: "Sin problemas".to_string(),

            findings: Vec::new(),
        }
    }

    fn anomalies() -> AnomalyReport {
        AnomalyReport {
            anomalies: Vec::new(),

            deploy_ready: true,

            summary: "Sin anomalías".to_string(),
        }
    }

    fn checklist(items: Vec<ChecklistItem>) -> DeployChecklist {
        let passed = items
            .iter()
            .filter(|item| item.state == CheckState::Passed)
            .count();

        let warnings = items
            .iter()
            .filter(|item| item.state == CheckState::Warning)
            .count();

        let failed = items
            .iter()
            .filter(|item| item.state == CheckState::Failed)
            .count();

        DeployChecklist {
            items,
            passed,
            warnings,
            failed,
            ready: failed == 0,
            summary: String::new(),
        }
    }

    #[test]
    fn development_policy_allows_dirty_tree() {
        let checklist = checklist(vec![ChecklistItem {
            code: "WORKING_TREE_CLEAN".to_string(),

            title: "Existen cambios locales".to_string(),

            detail: "Hay archivos modificados.".to_string(),

            state: CheckState::Failed,
        }]);

        let policy = DeployPolicy::from_preset(PolicyPreset::Development);

        let gate = evaluate_deploy_gate(
            "Demo",
            &status(),
            &health(),
            &diagnosis(80),
            &anomalies(),
            &checklist,
            &policy,
        );

        assert!(gate.ready);

        assert_eq!(gate.decision, GateDecision::ApprovedWithWarnings);
    }

    #[test]
    fn production_policy_blocks_warnings() {
        let checklist = checklist(vec![ChecklistItem {
            code: "UPSTREAM_CONFIGURED".to_string(),

            title: "La rama no tiene upstream".to_string(),

            detail: "No se puede comprobar sincronización.".to_string(),

            state: CheckState::Warning,
        }]);

        let policy = DeployPolicy::from_preset(PolicyPreset::Production);

        let gate = evaluate_deploy_gate(
            "Demo",
            &status(),
            &health(),
            &diagnosis(100),
            &anomalies(),
            &checklist,
            &policy,
        );

        assert!(!gate.ready);

        assert_eq!(gate.decision, GateDecision::Blocked);
    }

    #[test]
    fn minimum_score_blocks_deploy() {
        let checklist = checklist(Vec::new());

        let policy = DeployPolicy::from_preset(PolicyPreset::Production);

        let gate = evaluate_deploy_gate(
            "Demo",
            &status(),
            &health(),
            &diagnosis(70),
            &anomalies(),
            &checklist,
            &policy,
        );

        assert!(!gate.ready);

        assert!(
            gate.blockers
                .iter()
                .any(|blocker| { blocker.contains("POLICY_MINIMUM_SCORE",) })
        );
    }

    #[test]
    fn creates_safe_manifest_filename() {
        assert_eq!(
            suggested_gate_filename("Kuali Web",),
            "kuali-web-deploy-gate.json"
        );
    }

    #[test]
    fn policy_presets_have_expected_values() {
        let development = DeployPolicy::from_preset(PolicyPreset::Development);

        let production = DeployPolicy::from_preset(PolicyPreset::Production);

        assert!(!development.require_clean_tree);

        assert!(production.require_health);

        assert!(production.strict_warnings);

        assert_eq!(production.minimum_score, 90);
    }
}
