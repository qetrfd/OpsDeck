use eframe::egui;
use opsdeck::anomaly::AnomalyReport;
use opsdeck::checklist::{CheckState, DeployChecklist};
use opsdeck::health::{HealthCheck, HealthState};
use opsdeck::history::{FeedbackSummary, ReviewRecord, feedback_summary, record_feedback};
use opsdeck::history_ui::show_history_panel;
use opsdeck::intelligence::Diagnosis;
use opsdeck::monitor::{MonitorEvent, MonitorHandle, MonitorResult, spawn_monitor_worker};
use opsdeck::report::{export_deploy_report, suggested_report_filename};
use opsdeck::{
    Project, ProjectStatus, add_project, config_path, load_config, open_in_file_manager,
    open_in_vscode, save_config,
};
use rfd::FileDialog;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant, SystemTime};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OpsDeck")
            .with_inner_size([1220.0, 790.0])
            .with_min_inner_size([920.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "OpsDeck",
        options,
        Box::new(|creation_context| Ok(Box::new(OpsDeckApp::new(creation_context)))),
    )
}

#[derive(Clone)]
struct ProjectSnapshot {
    status: Option<ProjectStatus>,
    health: HealthCheck,
    diagnosis: Option<Diagnosis>,
    history: Vec<ReviewRecord>,
    anomaly_report: AnomalyReport,
    checklist: DeployChecklist,
    feedback: HashMap<String, FeedbackSummary>,
    error: Option<String>,
    history_error: Option<String>,
    checked_at: SystemTime,
}

impl From<MonitorResult> for ProjectSnapshot {
    fn from(result: MonitorResult) -> Self {
        let feedback = load_feedback_map(&result.project_name, result.diagnosis.as_ref());

        Self {
            status: result.status,
            health: result.health,
            diagnosis: result.diagnosis,
            history: result.history,
            anomaly_report: result.anomaly_report,
            checklist: result.checklist,
            feedback,
            error: result.error,
            history_error: result.history_error,
            checked_at: result.checked_at,
        }
    }
}

struct OpsDeckApp {
    projects: Vec<Project>,
    snapshots: HashMap<String, ProjectSnapshot>,
    checking_projects: HashSet<String>,
    selected_name: Option<String>,
    notice: Option<String>,
    show_add_dialog: bool,
    new_project_name: String,
    new_project_path: String,
    new_health_url: String,
    delete_target: Option<String>,
    auto_refresh: bool,
    refresh_interval_secs: u64,
    last_check_request: Instant,
    last_result_received: Option<Instant>,
    monitor: Option<MonitorHandle>,
}

impl OpsDeckApp {
    fn new(_creation_context: &eframe::CreationContext<'_>) -> Self {
        let monitor_result = spawn_monitor_worker();

        let mut app = Self {
            projects: Vec::new(),
            snapshots: HashMap::new(),
            checking_projects: HashSet::new(),
            selected_name: None,
            notice: None,
            show_add_dialog: false,
            new_project_name: String::new(),
            new_project_path: String::new(),
            new_health_url: String::new(),
            delete_target: None,
            auto_refresh: true,
            refresh_interval_secs: 60,
            last_check_request: Instant::now(),
            last_result_received: None,
            monitor: None,
        };

        match monitor_result {
            Ok(monitor) => {
                app.monitor = Some(monitor);
            }

            Err(error) => {
                app.notice = Some(error);
            }
        }

        app.reload_projects();
        app
    }

    fn reload_projects(&mut self) {
        match load_config() {
            Ok(config) => {
                self.projects = config.projects;

                let project_names = self
                    .projects
                    .iter()
                    .map(|project| project.name.to_lowercase())
                    .collect::<HashSet<_>>();

                self.snapshots
                    .retain(|name, _| project_names.contains(&name.to_lowercase()));

                self.checking_projects
                    .retain(|name| project_names.contains(&name.to_lowercase()));

                let selected_exists = self
                    .selected_name
                    .as_ref()
                    .map(|name| project_names.contains(&name.to_lowercase()))
                    .unwrap_or(false);

                if !selected_exists {
                    self.selected_name = self.projects.first().map(|project| project.name.clone());
                }

                self.request_all_checks();
            }

            Err(error) => {
                self.projects.clear();
                self.snapshots.clear();
                self.checking_projects.clear();
                self.selected_name = None;
                self.notice = Some(error);
            }
        }
    }

    fn request_all_checks(&mut self) {
        if self.projects.is_empty() {
            return;
        }

        if !self.checking_projects.is_empty() {
            self.notice = Some("Ya hay una revisión en curso".to_string());
            return;
        }

        let projects = self.projects.clone();

        let names = projects
            .iter()
            .map(|project| project.name.clone())
            .collect::<Vec<_>>();

        for name in &names {
            self.checking_projects.insert(name.clone());
        }

        let result = match &self.monitor {
            Some(monitor) => monitor.check_all(projects),

            None => Err("El monitor en segundo plano no está disponible".to_string()),
        };

        match result {
            Ok(()) => {
                self.last_check_request = Instant::now();
            }

            Err(error) => {
                for name in names {
                    self.checking_projects.remove(&name);
                }

                self.notice = Some(error);
            }
        }
    }

    fn request_project_check(&mut self, project_name: &str) {
        let Some(project) = self
            .projects
            .iter()
            .find(|project| project.name.eq_ignore_ascii_case(project_name))
            .cloned()
        else {
            self.notice = Some(format!("No se encontró el proyecto {project_name}"));
            return;
        };

        if self.checking_projects.contains(&project.name) {
            self.notice = Some(format!("{} ya se está revisando", project.name));
            return;
        }

        self.checking_projects.insert(project.name.clone());

        let result = match &self.monitor {
            Some(monitor) => monitor.check_project(project.clone()),

            None => Err("El monitor en segundo plano no está disponible".to_string()),
        };

        match result {
            Ok(()) => {
                self.last_check_request = Instant::now();
            }

            Err(error) => {
                self.checking_projects.remove(&project.name);

                self.notice = Some(error);
            }
        }
    }

    fn process_monitor_events(&mut self) {
        let mut events = Vec::new();
        let mut disconnected = false;

        if let Some(monitor) = self.monitor.as_ref() {
            loop {
                match monitor.try_recv() {
                    Ok(event) => events.push(event),

                    Err(TryRecvError::Empty) => break,

                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }

        if disconnected {
            self.monitor = None;
            self.checking_projects.clear();

            self.notice = Some("El worker de monitoreo se desconectó".to_string());
        }

        for event in events {
            self.handle_monitor_event(event);
        }
    }

    fn handle_monitor_event(&mut self, event: MonitorEvent) {
        match event {
            MonitorEvent::Started { project_name } => {
                self.checking_projects.insert(project_name);
            }

            MonitorEvent::Finished(result) => {
                let result = *result;
                let project_name = result.project_name.clone();

                self.checking_projects.remove(&project_name);

                self.last_result_received = Some(Instant::now());

                let project_exists = self
                    .projects
                    .iter()
                    .any(|project| project.name.eq_ignore_ascii_case(&project_name));

                if project_exists {
                    self.snapshots
                        .insert(project_name, ProjectSnapshot::from(result));
                }
            }
        }
    }

    fn selected_snapshot(&self) -> Option<ProjectSnapshot> {
        let name = self.selected_name.as_ref()?;
        self.snapshots.get(name).cloned()
    }

    fn save_new_project(&mut self) {
        let name = self.new_project_name.trim().to_string();

        let path = self.new_project_path.trim().to_string();

        let health_url = self.new_health_url.trim().to_string();

        if name.is_empty() {
            self.notice = Some("Escribe un nombre para el proyecto".to_string());
            return;
        }

        if path.is_empty() {
            self.notice = Some("Selecciona la carpeta del proyecto".to_string());
            return;
        }

        let health_url = if health_url.is_empty() {
            None
        } else {
            Some(health_url)
        };

        match add_project(name, PathBuf::from(path), health_url) {
            Ok(project) => {
                let project_name = project.name.clone();

                self.new_project_name.clear();
                self.new_project_path.clear();
                self.new_health_url.clear();
                self.show_add_dialog = false;
                self.selected_name = Some(project_name.clone());

                self.reload_projects();

                self.notice = Some(format!(
                    "El proyecto {project_name} fue registrado correctamente"
                ));
            }

            Err(error) => {
                self.notice = Some(error);
            }
        }
    }

    fn delete_project(&mut self, name: &str) {
        let result = (|| -> Result<(), String> {
            let mut config = load_config()?;
            let previous_count = config.projects.len();

            config
                .projects
                .retain(|project| !project.name.eq_ignore_ascii_case(name));

            if config.projects.len() == previous_count {
                return Err(format!("No se encontró el proyecto {name}"));
            }

            save_config(&config)?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                if self
                    .selected_name
                    .as_ref()
                    .map(|selected| selected.eq_ignore_ascii_case(name))
                    .unwrap_or(false)
                {
                    self.selected_name = None;
                }

                self.snapshots.remove(name);
                self.checking_projects.remove(name);
                self.reload_projects();

                self.notice = Some(format!("El proyecto {name} fue eliminado de OpsDeck"));
            }

            Err(error) => {
                self.notice = Some(error);
            }
        }
    }

    fn save_rule_feedback(&mut self, project_name: &str, rule_code: &str, useful: bool) {
        match record_feedback(project_name, rule_code, useful) {
            Ok(_) => {
                if let Some(snapshot) = self.snapshots.get_mut(project_name) {
                    let summary = snapshot.feedback.entry(rule_code.to_string()).or_default();

                    if useful {
                        summary.useful += 1;
                    } else {
                        summary.not_useful += 1;
                    }
                }

                let answer = if useful { "útil" } else { "no útil" };

                self.notice = Some(format!(
                    "Retroalimentación guardada: {rule_code} fue marcada como {answer}"
                ));

                self.request_project_check(project_name);
            }

            Err(error) => {
                self.notice = Some(error);
            }
        }
    }

    fn export_selected_report(&mut self, project_name: &str, snapshot: &ProjectSnapshot) {
        let Some(status) = snapshot.status.as_ref() else {
            self.notice = Some("No hay información del repositorio para exportar".to_string());
            return;
        };

        let Some(diagnosis) = snapshot.diagnosis.as_ref() else {
            self.notice = Some("No hay diagnóstico para exportar".to_string());
            return;
        };

        let filename = suggested_report_filename(project_name);

        let Some(path) = FileDialog::new()
            .set_title("Guardar informe de deploy")
            .set_file_name(&filename)
            .add_filter("Markdown", &["md"])
            .save_file()
        else {
            return;
        };

        match export_deploy_report(
            project_name,
            status,
            &snapshot.health,
            diagnosis,
            &snapshot.history,
            &snapshot.anomaly_report,
            Some(path.as_path()),
        ) {
            Ok(path) => {
                self.notice = Some(format!("Informe guardado en {}", path.display()));
            }

            Err(error) => {
                self.notice = Some(error);
            }
        }
    }

    fn open_selected_in_vscode(&mut self, status: &ProjectStatus) {
        match open_in_vscode(&status.path) {
            Ok(()) => {
                self.notice = Some(format!("{} se abrió en Visual Studio Code", status.name));
            }

            Err(error) => {
                self.notice = Some(error);
            }
        }
    }

    fn open_selected_folder(&mut self, status: &ProjectStatus) {
        match open_in_file_manager(&status.path) {
            Ok(()) => {
                self.notice = Some(format!("Carpeta abierta: {}", status.path.display()));
            }

            Err(error) => {
                self.notice = Some(error);
            }
        }
    }

    fn show_header(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading(egui::RichText::new("OpsDeck").size(27.0).strong());

                ui.label("Centro de control local para tus proyectos");
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Agregar proyecto").clicked() {
                    self.show_add_dialog = true;
                }

                if ui.button("Recargar configuración").clicked() {
                    self.reload_projects();

                    self.notice = Some("Configuración recargada".to_string());
                }

                if ui.button("Revisar todos").clicked() {
                    self.request_all_checks();
                }
            });
        });

        ui.add_space(5.0);

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.auto_refresh, "Revisión automática");

            ui.add(
                egui::Slider::new(&mut self.refresh_interval_secs, 15..=900)
                    .text("intervalo en segundos"),
            );

            ui.separator();

            if self.checking_projects.is_empty() {
                ui.label("Monitor disponible");
            } else {
                ui.spinner();

                ui.label(format!(
                    "Revisando {} proyecto(s)",
                    self.checking_projects.len()
                ));
            }

            ui.separator();

            match self.last_result_received {
                Some(last_result) => {
                    ui.label(format!(
                        "Último resultado: hace {} s",
                        last_result.elapsed().as_secs()
                    ));
                }

                None => {
                    ui.label("Esperando primer resultado");
                }
            }
        });

        ui.add_space(8.0);
    }

    fn show_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.heading("Proyectos");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("+").clicked() {
                    self.show_add_dialog = true;
                }
            });
        });

        ui.separator();

        if self.projects.is_empty() {
            ui.label("No hay proyectos registrados.");
            ui.add_space(8.0);

            if ui.button("Agregar primer proyecto").clicked() {
                self.show_add_dialog = true;
            }

            return;
        }

        let projects = self.projects.clone();
        let mut selected_project = None;
        let mut project_to_delete = None;
        let mut project_to_check = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for project in projects {
                    let is_selected = self
                        .selected_name
                        .as_ref()
                        .map(|name| name.eq_ignore_ascii_case(&project.name))
                        .unwrap_or(false);

                    let is_checking = self.checking_projects.contains(&project.name);

                    let snapshot = self.snapshots.get(&project.name).cloned();

                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            let response = ui.selectable_label(
                                is_selected,
                                egui::RichText::new(&project.name).strong(),
                            );

                            if response.clicked() {
                                selected_project = Some(project.name.clone());
                            }

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("Eliminar").clicked() {
                                        project_to_delete = Some(project.name.clone());
                                    }
                                },
                            );
                        });

                        ui.small(project.path.display().to_string());

                        if is_checking {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Revisando...");
                            });
                        } else if let Some(snapshot) = snapshot {
                            if let Some(diagnosis) = snapshot.diagnosis.as_ref() {
                                ui.label(format!("{} · {}/100", diagnosis.risk, diagnosis.score));
                            } else if snapshot.error.is_some() {
                                ui.label("Error durante la revisión");
                            } else {
                                ui.label("Sin diagnóstico");
                            }

                            ui.small(format!("Health: {}", snapshot.health.state));

                            ui.small(format!("Historial: {} registros", snapshot.history.len()));

                            ui.small(if snapshot.checklist.ready {
                                if snapshot.checklist.warnings > 0 {
                                    "Deploy: permitido con advertencias"
                                } else {
                                    "Deploy: aprobado"
                                }
                            } else {
                                "Deploy: bloqueado"
                            });
                        } else {
                            ui.label("Sin revisar");
                        }

                        if ui.small_button("Revisar ahora").clicked() {
                            project_to_check = Some(project.name.clone());
                        }
                    });

                    ui.add_space(7.0);
                }
            });

        if let Some(name) = selected_project {
            self.selected_name = Some(name);
        }

        if let Some(name) = project_to_delete {
            self.delete_target = Some(name);
        }

        if let Some(name) = project_to_check {
            self.request_project_check(&name);
        }

        ui.separator();

        if let Ok(path) = config_path() {
            ui.small(format!("Configuración: {}", path.display()));
        }
    }

    fn show_content(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);

        if let Some(notice) = self.notice.clone() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(notice);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Cerrar").clicked() {
                            self.notice = None;
                        }
                    });
                });
            });

            ui.add_space(8.0);
        }

        let Some(selected_name) = self.selected_name.clone() else {
            ui.vertical_centered(|ui| {
                ui.add_space(120.0);
                ui.heading("No hay un proyecto seleccionado");

                ui.label("Agrega o selecciona un proyecto en la barra lateral.");

                ui.add_space(10.0);

                if ui.button("Agregar proyecto").clicked() {
                    self.show_add_dialog = true;
                }
            });

            return;
        };

        let is_checking = self.checking_projects.contains(&selected_name);

        let Some(snapshot) = self.selected_snapshot() else {
            ui.vertical_centered(|ui| {
                ui.add_space(100.0);
                ui.heading(&selected_name);

                if is_checking {
                    ui.spinner();
                    ui.label("Revisando proyecto...");
                } else {
                    ui.label("Este proyecto todavía no ha sido revisado.");

                    if ui.button("Revisar ahora").clicked() {
                        self.request_project_check(&selected_name);
                    }
                }
            });

            return;
        };

        if let Some(error) = snapshot.error.clone() {
            ui.heading(&selected_name);
            ui.add_space(10.0);

            ui.label(egui::RichText::new("No se pudo revisar el repositorio").strong());

            ui.label(error);
            ui.add_space(10.0);

            if ui.button("Intentar nuevamente").clicked() {
                self.request_project_check(&selected_name);
            }

            return;
        }

        let Some(status) = snapshot.status.clone() else {
            ui.label("No hay información de Git disponible.");
            return;
        };

        let Some(diagnosis) = snapshot.diagnosis.clone() else {
            ui.label("No hay diagnóstico disponible.");
            return;
        };

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.heading(egui::RichText::new(&status.name).size(25.0).strong());

                        ui.monospace(status.path.display().to_string());

                        ui.small(format!(
                            "Última revisión: hace {} s",
                            seconds_since(&snapshot.checked_at,)
                        ));
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Exportar informe").clicked() {
                            self.export_selected_report(&selected_name, &snapshot);
                        }

                        if ui.button("Abrir carpeta").clicked() {
                            self.open_selected_folder(&status);
                        }

                        if ui.button("Abrir en VS Code").clicked() {
                            self.open_selected_in_vscode(&status);
                        }

                        if ui
                            .add_enabled(!is_checking, egui::Button::new("Revisar ahora"))
                            .clicked()
                        {
                            self.request_project_check(&selected_name);
                        }

                        if is_checking {
                            ui.spinner();
                        }
                    });
                });

                ui.add_space(10.0);

                ui.group(|ui| {
                    ui.heading(status.state_label());
                    ui.add_space(8.0);

                    egui::Grid::new("repository_information")
                        .num_columns(2)
                        .spacing([28.0, 10.0])
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("Rama").strong());
                            ui.monospace(&status.branch);
                            ui.end_row();

                            ui.label(egui::RichText::new("Último commit").strong());
                            ui.label(&status.last_commit);
                            ui.end_row();

                            ui.label(egui::RichText::new("Remoto").strong());
                            ui.monospace(&status.remote);
                            ui.end_row();

                            ui.label(egui::RichText::new("Upstream").strong());
                            ui.monospace(status.sync.upstream.as_deref().unwrap_or("Sin upstream"));
                            ui.end_row();

                            ui.label(egui::RichText::new("Commits por subir").strong());
                            ui.label(status.sync.ahead.to_string());
                            ui.end_row();

                            ui.label(egui::RichText::new("Commits por descargar").strong());
                            ui.label(status.sync.behind.to_string());
                            ui.end_row();
                        });
                });

                ui.add_space(10.0);

                ui.horizontal_wrapped(|ui| {
                    status_card(ui, "Cambios", status.changes.total);
                    status_card(ui, "Preparados", status.changes.staged);
                    status_card(ui, "Sin preparar", status.changes.unstaged);
                    status_card(ui, "Nuevos", status.changes.untracked);
                    status_card(ui, "Por subir", status.sync.ahead);
                    status_card(ui, "Por descargar", status.sync.behind);
                });

                ui.add_space(10.0);

                show_health_panel(ui, &snapshot.health);

                ui.add_space(10.0);

                show_history_panel(ui, &snapshot.history);

                ui.add_space(10.0);

                show_anomaly_panel(ui, &snapshot.anomaly_report);

                ui.add_space(10.0);

                show_checklist_panel(ui, &snapshot.checklist);

                if let Some(error) = &snapshot.history_error {
                    ui.add_space(5.0);

                    ui.label(
                        egui::RichText::new(format!("No se pudo actualizar el historial: {error}"))
                            .strong(),
                    );
                }

                ui.add_space(10.0);

                let mut feedback_action: Option<(String, bool)> = None;

                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.heading("OpsDeck Intelligence");

                            ui.label(egui::RichText::new(diagnosis.risk.to_string()).strong());
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!("{}/100", diagnosis.score))
                                    .size(28.0)
                                    .strong(),
                            );
                        });
                    });

                    ui.add_space(6.0);
                    ui.label(&diagnosis.summary);
                    ui.add_space(8.0);

                    if diagnosis.findings.is_empty() {
                        ui.label("No se encontraron problemas.");
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(320.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for finding in &diagnosis.findings {
                                    let summary = snapshot
                                        .feedback
                                        .get(&finding.code)
                                        .copied()
                                        .unwrap_or_default();

                                    ui.collapsing(
                                        format!("{} · {}", finding.severity.label(), finding.title),
                                        |ui| {
                                            ui.label(&finding.explanation);

                                            ui.add_space(5.0);

                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "Acción recomendada: {}",
                                                    finding.action
                                                ))
                                                .strong(),
                                            );

                                            ui.add_space(3.0);

                                            ui.small(format!(
                                                "Regla: {} · Penalización adaptada: -{}",
                                                finding.code, finding.penalty
                                            ));

                                            ui.add_space(7.0);

                                            ui.horizontal(|ui| {
                                                if ui.small_button("Útil").clicked() {
                                                    feedback_action =
                                                        Some((finding.code.clone(), true));
                                                }

                                                if ui.small_button("No útil").clicked() {
                                                    feedback_action =
                                                        Some((finding.code.clone(), false));
                                                }

                                                ui.small(format!(
                                                    "Útil: {} · No útil: {}",
                                                    summary.useful, summary.not_useful
                                                ));
                                            });
                                        },
                                    );
                                }
                            });
                    }
                });

                if let Some((rule_code, useful)) = feedback_action {
                    self.save_rule_feedback(&selected_name, &rule_code, useful);
                }

                ui.add_space(10.0);
                ui.heading("Cambios locales");
                ui.separator();

                if status.raw_status.trim().is_empty() {
                    ui.label("No hay cambios locales pendientes.");
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(230.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.monospace(&status.raw_status);
                        });
                }

                ui.add_space(20.0);
            });
    }

    fn show_add_window(&mut self, context: &egui::Context) {
        if !self.show_add_dialog {
            return;
        }

        let mut open = self.show_add_dialog;
        let mut save_clicked = false;
        let mut cancel_clicked = false;

        egui::Window::new("Agregar proyecto")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .show(context, |ui| {
                ui.label("Nombre del proyecto");

                ui.text_edit_singleline(&mut self.new_project_name);

                ui.add_space(8.0);
                ui.label("Carpeta del repositorio");

                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.new_project_path);

                    if ui.button("Seleccionar").clicked()
                        && let Some(path) = FileDialog::new()
                            .set_title("Selecciona el repositorio Git")
                            .pick_folder()
                    {
                        let fill_name = self.new_project_name.trim().is_empty();

                        self.new_project_path = path.display().to_string();

                        if fill_name
                            && let Some(name) = path.file_name().and_then(|value| value.to_str())
                        {
                            self.new_project_name = name.to_string();
                        }
                    }
                });

                ui.add_space(8.0);
                ui.label("Health URL opcional");

                ui.text_edit_singleline(&mut self.new_health_url);

                ui.add_space(5.0);

                ui.small("La carpeta debe contener un repositorio Git válido.");

                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    let can_save = !self.new_project_name.trim().is_empty()
                        && !self.new_project_path.trim().is_empty();

                    if ui
                        .add_enabled(can_save, egui::Button::new("Guardar proyecto"))
                        .clicked()
                    {
                        save_clicked = true;
                    }

                    if ui.button("Cancelar").clicked() {
                        cancel_clicked = true;
                    }
                });
            });

        if cancel_clicked {
            open = false;
        }

        self.show_add_dialog = open;

        if save_clicked {
            self.save_new_project();
        }
    }

    fn show_delete_window(&mut self, context: &egui::Context) {
        let Some(project_name) = self.delete_target.clone() else {
            return;
        };

        let mut confirm_clicked = false;
        let mut cancel_clicked = false;

        egui::Window::new("Eliminar proyecto")
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(context, |ui| {
                ui.label(format!(
                    "¿Quieres eliminar {project_name} de OpsDeck?"
                ));

                ui.add_space(5.0);

                ui.label(
                    "Esto solamente lo quitará de la lista. Los archivos y el repositorio no serán eliminados.",
                );

                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    if ui.button("Eliminar").clicked() {
                        confirm_clicked = true;
                    }

                    if ui.button("Cancelar").clicked() {
                        cancel_clicked = true;
                    }
                });
            });

        if confirm_clicked {
            self.delete_project(&project_name);
            self.delete_target = None;
        } else if cancel_clicked {
            self.delete_target = None;
        }
    }
}

impl eframe::App for OpsDeckApp {
    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_monitor_events();

        let interval = Duration::from_secs(self.refresh_interval_secs.max(15));

        if self.auto_refresh
            && self.last_check_request.elapsed() >= interval
            && self.checking_projects.is_empty()
        {
            self.request_all_checks();
        }

        context.request_repaint_after(Duration::from_millis(250));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.style_mut().spacing.item_spacing = egui::vec2(10.0, 10.0);

        ui.style_mut().spacing.button_padding = egui::vec2(14.0, 8.0);

        let context = ui.ctx().clone();

        egui::Panel::top("header").resizable(false).show(ui, |ui| {
            self.show_header(ui);
        });

        egui::Panel::left("projects")
            .resizable(true)
            .default_size(310.0)
            .min_size(250.0)
            .max_size(450.0)
            .show(ui, |ui| {
                self.show_sidebar(ui);
            });

        egui::CentralPanel::default().show(ui, |ui| {
            self.show_content(ui);
        });

        self.show_add_window(&context);
        self.show_delete_window(&context);
    }
}

fn show_health_panel(ui: &mut egui::Ui, health: &HealthCheck) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading("Health");

                ui.label(egui::RichText::new(health.state.to_string()).strong());
            });

            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| match health.state {
                    HealthState::Healthy => {
                        ui.label("Disponible");
                    }

                    HealthState::Degraded => {
                        ui.label("Atención");
                    }

                    HealthState::NotConfigured => {
                        ui.label("Sin endpoint");
                    }

                    _ => {
                        ui.label("Problema detectado");
                    }
                },
            );
        });

        ui.add_space(6.0);

        egui::Grid::new("health_information")
            .num_columns(2)
            .spacing([28.0, 9.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("URL").strong());

                ui.monospace(health.url.as_deref().unwrap_or("Sin configurar"));
                ui.end_row();

                ui.label(egui::RichText::new("Código HTTP").strong());

                ui.label(
                    health
                        .status_code
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "No disponible".to_string()),
                );
                ui.end_row();

                ui.label(egui::RichText::new("Latencia").strong());

                ui.label(
                    health
                        .latency_ms
                        .map(|latency| format!("{latency} ms"))
                        .unwrap_or_else(|| "No disponible".to_string()),
                );
                ui.end_row();

                ui.label(egui::RichText::new("Content-Type").strong());

                ui.monospace(health.content_type.as_deref().unwrap_or("No disponible"));
                ui.end_row();

                ui.label(egui::RichText::new("JSON válido").strong());

                let json_label = match health.json_valid {
                    Some(true) => "Sí",
                    Some(false) => "No",
                    None => "No aplica",
                };

                ui.label(json_label);
                ui.end_row();
            });

        if let Some(error) = &health.error {
            ui.add_space(8.0);

            ui.label(egui::RichText::new(format!("Error: {error}")).strong());
        }

        if let Some(preview) = &health.body_preview {
            ui.add_space(8.0);

            ui.collapsing("Vista previa de la respuesta", |ui| {
                egui::ScrollArea::vertical()
                    .max_height(130.0)
                    .show(ui, |ui| {
                        ui.monospace(preview);
                    });
            });
        }
    });
}

fn show_anomaly_panel(ui: &mut egui::Ui, report: &AnomalyReport) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading("Análisis de anomalías");

                ui.label(
                    egui::RichText::new(if report.deploy_ready {
                        "Listo para deploy"
                    } else {
                        "Deploy no recomendado"
                    })
                    .strong(),
                );
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("{} anomalía(s)", report.anomalies.len()));
            });
        });

        ui.add_space(6.0);
        ui.label(&report.summary);

        if report.anomalies.is_empty() {
            return;
        }

        ui.add_space(8.0);

        for anomaly in &report.anomalies {
            ui.collapsing(format!("{} · {}", anomaly.severity, anomaly.title), |ui| {
                ui.label(&anomaly.explanation);

                ui.add_space(5.0);

                ui.label(
                    egui::RichText::new(format!("Acción recomendada: {}", anomaly.action)).strong(),
                );

                ui.add_space(3.0);

                ui.small(format!("Código: {}", anomaly.code));
            });
        }
    });
}

fn show_checklist_panel(ui: &mut egui::Ui, checklist: &DeployChecklist) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading("Lista previa al deploy");

                ui.label(
                    egui::RichText::new(if checklist.ready {
                        if checklist.warnings > 0 {
                            "Permitido con advertencias"
                        } else {
                            "Deploy aprobado"
                        }
                    } else {
                        "Deploy bloqueado"
                    })
                    .strong(),
                );
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!(
                    "{} aprobados · {} advertencias · {} bloqueados",
                    checklist.passed, checklist.warnings, checklist.failed
                ));
            });
        });

        ui.add_space(6.0);
        ui.label(&checklist.summary);
        ui.add_space(8.0);

        for item in &checklist.items {
            let symbol = match item.state {
                CheckState::Passed => "✓",
                CheckState::Warning => "!",
                CheckState::Failed => "×",
            };

            ui.collapsing(format!("{symbol} {} · {}", item.state, item.title), |ui| {
                ui.label(&item.detail);
                ui.add_space(3.0);

                ui.small(format!("Código: {}", item.code));
            });
        }
    });
}

fn status_card(ui: &mut egui::Ui, label: &str, value: usize) {
    ui.group(|ui| {
        ui.set_min_width(118.0);

        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(value.to_string()).size(24.0).strong());

            ui.label(label);
        });
    });
}

fn seconds_since(time: &SystemTime) -> u64 {
    time.elapsed().unwrap_or_default().as_secs()
}

fn load_feedback_map(
    project_name: &str,
    diagnosis: Option<&Diagnosis>,
) -> HashMap<String, FeedbackSummary> {
    let mut feedback = HashMap::new();

    let Some(diagnosis) = diagnosis else {
        return feedback;
    };

    for finding in &diagnosis.findings {
        let summary = feedback_summary(project_name, &finding.code).unwrap_or_default();

        feedback.insert(finding.code.clone(), summary);
    }

    feedback
}
