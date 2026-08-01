use eframe::egui;
use opsdeck::anomaly::AnomalyReport;
use opsdeck::checklist::{CheckState, DeployChecklist};
use opsdeck::gate::{
    DeployGate, DeployPolicy, PolicyPreset, export_gate_manifest, load_policy, reset_policy,
    save_policy, suggested_gate_filename,
};
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
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([960.0, 640.0]),
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
    gate: DeployGate,
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
            gate: result.gate,
            feedback,
            error: result.error,
            history_error: result.history_error,
            checked_at: result.checked_at,
        }
    }
}

#[derive(Clone)]
struct PolicyEditorState {
    project_name: String,
    policy: DeployPolicy,
    max_latency_enabled: bool,
    max_latency_ms: u64,
}

impl PolicyEditorState {
    fn new(project_name: String, policy: DeployPolicy) -> Self {
        Self {
            project_name,
            max_latency_enabled: policy.max_latency_ms.is_some(),
            max_latency_ms: policy.max_latency_ms.unwrap_or(1_000),
            policy,
        }
    }

    fn policy_to_save(&self) -> DeployPolicy {
        let mut policy = self.policy.clone();
        policy.max_latency_ms = self
            .max_latency_enabled
            .then_some(self.max_latency_ms.max(1));
        policy
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
    policy_editor: Option<PolicyEditorState>,
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
            policy_editor: None,
            auto_refresh: true,
            refresh_interval_secs: 60,
            last_check_request: Instant::now(),
            last_result_received: None,
            monitor: None,
        };

        match monitor_result {
            Ok(monitor) => app.monitor = Some(monitor),
            Err(error) => app.notice = Some(error),
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

        self.checking_projects.extend(names.iter().cloned());

        let result = match &self.monitor {
            Some(monitor) => monitor.check_all(projects),
            None => Err("El monitor en segundo plano no está disponible".to_string()),
        };

        match result {
            Ok(()) => self.last_check_request = Instant::now(),

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
            Ok(()) => self.last_check_request = Instant::now(),

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

        let health_url = (!health_url.is_empty()).then_some(health_url);

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

            Err(error) => self.notice = Some(error),
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

            save_config(&config)
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

            Err(error) => self.notice = Some(error),
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

            Err(error) => self.notice = Some(error),
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

            Err(error) => self.notice = Some(error),
        }
    }

    fn export_selected_gate(&mut self, project_name: &str, snapshot: &ProjectSnapshot) {
        let filename = suggested_gate_filename(project_name);

        let Some(path) = FileDialog::new()
            .set_title("Guardar manifiesto del deploy gate")
            .set_file_name(&filename)
            .add_filter("JSON", &["json"])
            .save_file()
        else {
            return;
        };

        match export_gate_manifest(&snapshot.gate, Some(path.as_path())) {
            Ok(path) => {
                self.notice = Some(format!("Manifiesto guardado en {}", path.display()));
            }

            Err(error) => self.notice = Some(error),
        }
    }

    fn open_policy_editor(&mut self, project_name: &str) {
        match load_policy(project_name) {
            Ok(policy) => {
                self.policy_editor = Some(PolicyEditorState::new(project_name.to_string(), policy));
            }

            Err(error) => self.notice = Some(error),
        }
    }

    fn open_selected_in_vscode(&mut self, status: &ProjectStatus) {
        match open_in_vscode(&status.path) {
            Ok(()) => {
                self.notice = Some(format!("{} se abrió en Visual Studio Code", status.name));
            }

            Err(error) => self.notice = Some(error),
        }
    }

    fn open_selected_folder(&mut self, status: &ProjectStatus) {
        match open_in_file_manager(&status.path) {
            Ok(()) => {
                self.notice = Some(format!("Carpeta abierta: {}", status.path.display()));
            }

            Err(error) => self.notice = Some(error),
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
        let mut project_policy = None;

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
                        } else if let Some(snapshot) = snapshot.as_ref() {
                            if let Some(diagnosis) = snapshot.diagnosis.as_ref() {
                                ui.label(format!("{} · {}/100", diagnosis.risk, diagnosis.score));
                            } else if snapshot.error.is_some() {
                                ui.label("Error durante la revisión");
                            } else {
                                ui.label("Sin diagnóstico");
                            }

                            ui.small(format!("Health: {}", snapshot.health.state));

                            ui.small(format!("Gate: {}", snapshot.gate.decision));

                            ui.small(format!("Política: {}", snapshot.gate.policy.preset.label()));
                        } else {
                            ui.label("Sin revisar");
                        }

                        ui.horizontal(|ui| {
                            if ui.small_button("Revisar ahora").clicked() {
                                project_to_check = Some(project.name.clone());
                            }

                            if ui.small_button("Política").clicked() {
                                project_policy = Some(project.name.clone());
                            }
                        });
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

        if let Some(name) = project_policy {
            self.open_policy_editor(&name);
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
                            seconds_since(&snapshot.checked_at)
                        ));
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Exportar gate").clicked() {
                            self.export_selected_gate(&selected_name, &snapshot);
                        }

                        if ui.button("Exportar informe").clicked() {
                            self.export_selected_report(&selected_name, &snapshot);
                        }

                        if ui.button("Editar política").clicked() {
                            self.open_policy_editor(&selected_name);
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
                show_repository_panel(ui, &status);

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

                ui.add_space(10.0);
                show_gate_panel(ui, &snapshot.gate);

                if let Some(error) = &snapshot.history_error {
                    ui.add_space(5.0);

                    ui.label(
                        egui::RichText::new(format!("No se pudo actualizar el historial: {error}"))
                            .strong(),
                    );
                }

                ui.add_space(10.0);

                let mut feedback_action: Option<(String, bool)> = None;

                show_intelligence_panel(ui, &diagnosis, &snapshot.feedback, &mut feedback_action);

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

    fn show_policy_window(&mut self, context: &egui::Context) {
        let Some(mut editor) = self.policy_editor.take() else {
            return;
        };

        let mut open = true;
        let mut save_clicked = false;
        let mut reset_clicked = false;
        let mut cancel_clicked = false;

        egui::Window::new(format!("Política de deploy · {}", editor.project_name))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .show(context, |ui| {
                ui.label(
                    "Configura las condiciones que OpsDeck debe exigir antes de aprobar un deploy.",
                );

                ui.add_space(10.0);

                let previous_preset = editor.policy.preset;

                egui::Grid::new("policy_editor_grid")
                    .num_columns(2)
                    .spacing([24.0, 12.0])
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Preset").strong());

                        egui::ComboBox::from_id_salt("policy_preset_selector")
                            .selected_text(editor.policy.preset.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut editor.policy.preset,
                                    PolicyPreset::Development,
                                    "Desarrollo",
                                );

                                ui.selectable_value(
                                    &mut editor.policy.preset,
                                    PolicyPreset::Balanced,
                                    "Equilibrada",
                                );

                                ui.selectable_value(
                                    &mut editor.policy.preset,
                                    PolicyPreset::Production,
                                    "Producción",
                                );
                            });

                        ui.end_row();

                        if editor.policy.preset != previous_preset {
                            let policy = DeployPolicy::from_preset(editor.policy.preset);

                            editor.max_latency_enabled = policy.max_latency_ms.is_some();

                            editor.max_latency_ms = policy.max_latency_ms.unwrap_or(1_000);

                            editor.policy = policy;
                        }

                        ui.label(egui::RichText::new("Descripción").strong());

                        ui.label(editor.policy.preset.description());

                        ui.end_row();

                        ui.label(egui::RichText::new("Puntuación mínima").strong());

                        ui.add(
                            egui::DragValue::new(&mut editor.policy.minimum_score)
                                .range(0..=100)
                                .suffix("/100"),
                        );

                        ui.end_row();

                        ui.label(egui::RichText::new("Advertencias").strong());

                        ui.checkbox(
                            &mut editor.policy.strict_warnings,
                            "Bloquear cuando exista cualquier advertencia",
                        );

                        ui.end_row();

                        ui.label(egui::RichText::new("Health").strong());

                        ui.checkbox(
                            &mut editor.policy.require_health,
                            "Exigir un endpoint de health saludable",
                        );

                        ui.end_row();

                        ui.label(egui::RichText::new("Árbol de trabajo").strong());

                        ui.checkbox(
                            &mut editor.policy.require_clean_tree,
                            "Exigir que no existan cambios locales",
                        );

                        ui.end_row();

                        ui.label(egui::RichText::new("Commits locales").strong());

                        ui.checkbox(
                            &mut editor.policy.allow_commits_ahead,
                            "Permitir commits pendientes de subir",
                        );

                        ui.end_row();

                        ui.label(egui::RichText::new("Latencia máxima").strong());

                        ui.horizontal(|ui| {
                            ui.checkbox(&mut editor.max_latency_enabled, "Aplicar límite");

                            ui.add_enabled_ui(editor.max_latency_enabled, |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut editor.max_latency_ms)
                                        .range(1..=120_000)
                                        .speed(10)
                                        .suffix(" ms"),
                                );
                            });
                        });

                        ui.end_row();
                    });

                ui.add_space(10.0);
                ui.separator();

                ui.small(
                "Guardar la política vuelve a revisar el proyecto para recalcular el Deploy Gate.",
            );

                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if ui.button("Guardar política").clicked() {
                        save_clicked = true;
                    }

                    if ui.button("Restablecer balanced").clicked() {
                        reset_clicked = true;
                    }

                    if ui.button("Cancelar").clicked() {
                        cancel_clicked = true;
                    }
                });
            });

        if save_clicked {
            let project_name = editor.project_name.clone();
            let policy = editor.policy_to_save();

            match save_policy(&project_name, &policy) {
                Ok(path) => {
                    self.notice = Some(format!(
                        "Política {} guardada en {}",
                        policy.preset.label(),
                        path.display()
                    ));

                    self.request_project_check(&project_name);
                }

                Err(error) => {
                    self.notice = Some(error);
                    self.policy_editor = Some(editor);
                }
            }

            return;
        }

        if reset_clicked {
            let project_name = editor.project_name.clone();

            match reset_policy(&project_name) {
                Ok(_) => {
                    self.notice = Some(format!("{project_name} volvió a la política Equilibrada"));

                    self.request_project_check(&project_name);
                }

                Err(error) => {
                    self.notice = Some(error);
                    self.policy_editor = Some(editor);
                }
            }

            return;
        }

        if open && !cancel_clicked {
            self.policy_editor = Some(editor);
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
        self.show_policy_window(&context);
    }
}

fn show_repository_panel(ui: &mut egui::Ui, status: &ProjectStatus) {
    ui.group(|ui| {
        ui.heading(status.state_label());
        ui.add_space(8.0);

        egui::Grid::new("repository_information")
            .num_columns(2)
            .spacing([28.0, 10.0])
            .show(ui, |ui| {
                grid_row(ui, "Rama", &status.branch, true);

                grid_row(ui, "Último commit", &status.last_commit, false);

                grid_row(ui, "Remoto", &status.remote, true);

                grid_row(
                    ui,
                    "Upstream",
                    status.sync.upstream.as_deref().unwrap_or("Sin upstream"),
                    true,
                );

                grid_row(
                    ui,
                    "Commits por subir",
                    &status.sync.ahead.to_string(),
                    false,
                );

                grid_row(
                    ui,
                    "Commits por descargar",
                    &status.sync.behind.to_string(),
                    false,
                );
            });
    });
}

fn grid_row(ui: &mut egui::Ui, label: &str, value: &str, monospace: bool) {
    ui.label(egui::RichText::new(label).strong());

    if monospace {
        ui.monospace(value);
    } else {
        ui.label(value);
    }

    ui.end_row();
}

fn show_health_panel(ui: &mut egui::Ui, health: &HealthCheck) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading("Health");

                ui.label(egui::RichText::new(health.state.to_string()).strong());
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(match health.state {
                    HealthState::Healthy => "Disponible",
                    HealthState::Degraded => "Atención",
                    HealthState::NotConfigured => "Sin endpoint",
                    _ => "Problema detectado",
                });
            });
        });

        ui.add_space(6.0);

        egui::Grid::new("health_information")
            .num_columns(2)
            .spacing([28.0, 9.0])
            .show(ui, |ui| {
                grid_row(
                    ui,
                    "URL",
                    health.url.as_deref().unwrap_or("Sin configurar"),
                    true,
                );

                grid_row(
                    ui,
                    "Código HTTP",
                    &health
                        .status_code
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "No disponible".to_string()),
                    false,
                );

                grid_row(
                    ui,
                    "Latencia",
                    &health
                        .latency_ms
                        .map(|latency| format!("{latency} ms"))
                        .unwrap_or_else(|| "No disponible".to_string()),
                    false,
                );

                grid_row(
                    ui,
                    "Content-Type",
                    health.content_type.as_deref().unwrap_or("No disponible"),
                    true,
                );

                let json_label = match health.json_valid {
                    Some(true) => "Sí",
                    Some(false) => "No",
                    None => "No aplica",
                };

                grid_row(ui, "JSON válido", json_label, false);
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

        for anomaly in &report.anomalies {
            ui.collapsing(format!("{} · {}", anomaly.severity, anomaly.title), |ui| {
                ui.label(&anomaly.explanation);
                ui.add_space(5.0);

                ui.label(
                    egui::RichText::new(format!("Acción recomendada: {}", anomaly.action)).strong(),
                );

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

                ui.small(format!("Código: {}", item.code));
            });
        }
    });
}

fn show_gate_panel(ui: &mut egui::Ui, gate: &DeployGate) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading("Deploy Gate");

                ui.label(
                    egui::RichText::new(gate.decision.to_string())
                        .strong()
                        .size(18.0),
                );
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(if gate.ready {
                    "Operación permitida"
                } else {
                    "Operación bloqueada"
                });
            });
        });

        ui.add_space(6.0);
        ui.label(&gate.summary);
        ui.add_space(8.0);

        egui::Grid::new("gate_policy_information")
            .num_columns(2)
            .spacing([28.0, 8.0])
            .show(ui, |ui| {
                grid_row(ui, "Política", gate.policy.preset.label(), false);

                grid_row(
                    ui,
                    "Puntuación mínima",
                    &format!("{}/100", gate.policy.minimum_score),
                    false,
                );

                grid_row(
                    ui,
                    "Health obligatorio",
                    yes_no(gate.policy.require_health),
                    false,
                );

                grid_row(
                    ui,
                    "Árbol limpio",
                    yes_no(gate.policy.require_clean_tree),
                    false,
                );

                grid_row(
                    ui,
                    "Advertencias estrictas",
                    yes_no(gate.policy.strict_warnings),
                    false,
                );

                grid_row(
                    ui,
                    "Latencia máxima",
                    &gate
                        .policy
                        .max_latency_ms
                        .map(|value| format!("{value} ms"))
                        .unwrap_or_else(|| "Sin límite".to_string()),
                    false,
                );
            });

        if !gate.blockers.is_empty() {
            ui.add_space(8.0);

            ui.label(egui::RichText::new("Bloqueos").strong());

            for blocker in &gate.blockers {
                ui.label(format!("× {blocker}"));
            }
        }

        if !gate.warnings.is_empty() {
            ui.add_space(8.0);

            ui.label(egui::RichText::new("Advertencias").strong());

            for warning in &gate.warnings {
                ui.label(format!("! {warning}"));
            }
        }
    });
}

fn show_intelligence_panel(
    ui: &mut egui::Ui,
    diagnosis: &Diagnosis,
    feedback: &HashMap<String, FeedbackSummary>,
    feedback_action: &mut Option<(String, bool)>,
) {
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
            return;
        }

        egui::ScrollArea::vertical()
            .max_height(320.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for finding in &diagnosis.findings {
                    let summary = feedback.get(&finding.code).copied().unwrap_or_default();

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

                            ui.small(format!(
                                "Regla: {} · Penalización adaptada: -{}",
                                finding.code, finding.penalty
                            ));

                            ui.add_space(7.0);

                            ui.horizontal(|ui| {
                                if ui.small_button("Útil").clicked() {
                                    *feedback_action = Some((finding.code.clone(), true));
                                }

                                if ui.small_button("No útil").clicked() {
                                    *feedback_action = Some((finding.code.clone(), false));
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

fn yes_no(value: bool) -> &'static str {
    if value { "Sí" } else { "No" }
}
