use crate::ProjectStatus;
use crate::history::FeedbackSummary;
use crate::intelligence::{Diagnosis, FindingSeverity, RiskLevel};
use std::collections::HashMap;

pub fn apply_feedback(
    status: &ProjectStatus,
    mut diagnosis: Diagnosis,
    feedback: &HashMap<String, FeedbackSummary>,
) -> Diagnosis {
    let mut personalized_rules = 0;

    for finding in &mut diagnosis.findings {
        let key = finding.code.to_uppercase();

        let summary = feedback.get(&key).copied().unwrap_or_default();

        if summary.useful > 0 || summary.not_useful > 0 {
            personalized_rules += 1;

            finding.penalty = adjusted_penalty(finding.penalty, finding.severity, summary);
        }
    }

    let total_penalty = diagnosis
        .findings
        .iter()
        .map(|finding| finding.penalty as u16)
        .sum::<u16>()
        .min(100);

    diagnosis.score = 100_u8.saturating_sub(total_penalty as u8);

    let score_risk = risk_from_score(diagnosis.score);

    let finding_risk = diagnosis
        .findings
        .iter()
        .map(|finding| severity_risk(finding.severity))
        .max()
        .unwrap_or(RiskLevel::Healthy);

    diagnosis.risk = score_risk.max(finding_risk);

    diagnosis.summary = build_summary(
        status,
        diagnosis.score,
        diagnosis.risk,
        &diagnosis.findings,
        personalized_rules,
    );

    diagnosis
}

pub fn adjusted_penalty(
    base_penalty: u8,
    severity: FindingSeverity,
    feedback: FeedbackSummary,
) -> u8 {
    if base_penalty == 0 {
        return 0;
    }

    let multiplier = penalty_multiplier(severity, feedback);

    ((base_penalty as f32 * multiplier).round() as u16).min(100) as u8
}

pub fn penalty_multiplier(severity: FindingSeverity, feedback: FeedbackSummary) -> f32 {
    let total = feedback.useful + feedback.not_useful;

    if total == 0 {
        return 1.0;
    }

    let useful = feedback.useful as f32;
    let not_useful = feedback.not_useful as f32;
    let total = total as f32;

    let balance = (useful - not_useful) / total;
    let confidence = (total / 5.0).min(1.0);

    let calculated = 1.0 + (balance * confidence * 0.5);

    let minimum = match severity {
        FindingSeverity::Critical => 1.0,
        FindingSeverity::High => 0.8,
        FindingSeverity::Medium => 0.65,
        FindingSeverity::Low => 0.5,
        FindingSeverity::Info => 1.0,
    };

    calculated.clamp(minimum, 1.5)
}

fn severity_risk(severity: FindingSeverity) -> RiskLevel {
    match severity {
        FindingSeverity::Info => RiskLevel::Healthy,
        FindingSeverity::Low => RiskLevel::Low,
        FindingSeverity::Medium => RiskLevel::Medium,
        FindingSeverity::High => RiskLevel::High,
        FindingSeverity::Critical => RiskLevel::Critical,
    }
}

fn risk_from_score(score: u8) -> RiskLevel {
    match score {
        91..=100 => RiskLevel::Healthy,
        76..=90 => RiskLevel::Low,
        56..=75 => RiskLevel::Medium,
        31..=55 => RiskLevel::High,
        _ => RiskLevel::Critical,
    }
}

fn build_summary(
    status: &ProjectStatus,
    score: u8,
    risk: RiskLevel,
    findings: &[crate::intelligence::Finding],
    personalized_rules: usize,
) -> String {
    let base_summary = if findings.is_empty() {
        format!(
            "{} no presenta riesgos detectados y obtuvo una puntuación de {score}/100.",
            status.name
        )
    } else {
        let important = findings
            .iter()
            .filter(|finding| {
                matches!(
                    finding.severity,
                    FindingSeverity::High | FindingSeverity::Critical
                )
            })
            .count();

        match risk {
            RiskLevel::Healthy => format!(
                "{} se encuentra en buen estado con una puntuación de {score}/100.",
                status.name
            ),
            RiskLevel::Low => format!(
                "{} tiene detalles menores por revisar. Puntuación: {score}/100.",
                status.name
            ),
            RiskLevel::Medium => format!(
                "{} requiere atención antes del siguiente commit o deploy. Puntuación: {score}/100.",
                status.name
            ),
            RiskLevel::High => format!(
                "{} presenta {important} riesgos importantes. Evita desplegar hasta revisarlos. Puntuación: {score}/100.",
                status.name
            ),
            RiskLevel::Critical => format!(
                "{} presenta una condición crítica. Detén commits y despliegues hasta resolverla. Puntuación: {score}/100.",
                status.name
            ),
        }
    };

    if personalized_rules == 0 {
        base_summary
    } else if personalized_rules == 1 {
        format!("{base_summary} Se aplicó aprendizaje local a 1 regla.")
    } else {
        format!("{base_summary} Se aplicó aprendizaje local a {personalized_rules} reglas.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intelligence::{Diagnosis, Finding, FindingSeverity, RiskLevel};
    use crate::{ChangeSummary, ProjectStatus, SyncStatus};
    use std::path::PathBuf;

    fn project_status() -> ProjectStatus {
        ProjectStatus {
            name: "Demo".to_string(),
            path: PathBuf::from("/tmp/demo"),
            registered: true,
            health_url: None,
            branch: "main".to_string(),
            changes: ChangeSummary::default(),
            last_commit: "abc123".to_string(),
            remote: "origin".to_string(),
            sync: SyncStatus {
                upstream: Some("origin/main".to_string()),
                ahead: 0,
                behind: 0,
            },
            raw_status: String::new(),
        }
    }

    fn diagnosis(severity: FindingSeverity, penalty: u8) -> Diagnosis {
        Diagnosis {
            score: 100_u8.saturating_sub(penalty),
            risk: RiskLevel::Medium,
            summary: String::new(),
            findings: vec![Finding {
                code: "TEST_RULE".to_string(),
                title: "Regla de prueba".to_string(),
                explanation: "Prueba".to_string(),
                action: "Revisar".to_string(),
                severity,
                penalty,
            }],
        }
    }

    #[test]
    fn useful_feedback_increases_penalty() {
        let feedback = FeedbackSummary {
            useful: 5,
            not_useful: 0,
        };

        let adjusted = adjusted_penalty(20, FindingSeverity::Medium, feedback);

        assert_eq!(adjusted, 30);
    }

    #[test]
    fn negative_feedback_reduces_medium_penalty() {
        let feedback = FeedbackSummary {
            useful: 0,
            not_useful: 5,
        };

        let adjusted = adjusted_penalty(20, FindingSeverity::Medium, feedback);

        assert_eq!(adjusted, 13);
    }

    #[test]
    fn critical_penalty_cannot_be_reduced() {
        let feedback = FeedbackSummary {
            useful: 0,
            not_useful: 20,
        };

        let adjusted = adjusted_penalty(40, FindingSeverity::Critical, feedback);

        assert_eq!(adjusted, 40);
    }

    #[test]
    fn diagnosis_mentions_local_learning() {
        let mut feedback = HashMap::new();

        feedback.insert(
            "TEST_RULE".to_string(),
            FeedbackSummary {
                useful: 2,
                not_useful: 0,
            },
        );

        let result = apply_feedback(
            &project_status(),
            diagnosis(FindingSeverity::Medium, 20),
            &feedback,
        );

        assert!(result.summary.contains("aprendizaje local"));
    }
}
