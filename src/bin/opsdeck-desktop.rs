use eframe::egui;
use opsdeck::intelligence::{Diagnosis, analyze_project};
use opsdeck::{
    Project, ProjectStatus, add_project, config_path, load_config, open_in_file_manager,
    open_in_vscode, project_status, save_config,
};
use rfd::FileDialog;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OpsDeck")
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([900.0, 600.0]),
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
    diagnosis: Option<Diagnosis>,
    error: Option<String>,
}

impl ProjectSnapshot {
    fn success(status: ProjectStatus, diagnosis: Diagnosis) -> Self {
        Self {
            status: Some(status),
            diagnosis: Some(diagnosis),
            error: None,
        }
    }

    fn failure(error: String) -> Self {
        Self {
            status: None,
            diagnosis: None,
            error: Some(error),
        }
    }
}

struct OpsDeckApp {
    projects: Vec<Project>,
    snapshots: HashMap<String, ProjectSnapshot>,
    selected_name: Option<String>,
    notice: Option<String>,
    show_add_dialog: bool,
    new_project_name: String,
    new_project_path: String,
    new_health_url: String,
    delete_target: Option<String>,
    auto_refresh: bool,
    refresh_interval_secs: u64,
    last_refresh: Instant,
}

impl OpsDeckApp {
    fn new(_creation_context: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            projects: Vec::new(),
            snapshots: HashMap::new(),
            selected_name: None,
            notice: None,
            show_add_dialog: false,
            new_project_name: String::new(),
            new_project_path: String::new(),
            new_health_url: String::new(),
            delete_target: None,
            auto_refresh: true,
            refresh_interval_secs: 30,
            last_refresh: Instant::now(),
        };

        app.reload_projects();
        app
    }

    fn reload_projects(&mut self) {
        match load_config() {
            Ok(config) => {
                self.projects = config.projects;

                let selected_exists = self
                    .selected_name
                    .as_ref()
                    .map(|name| {
                        self.projects
                            .iter()
                            .any(|project| project.name.eq_ignore_ascii_case(name))
                    })
                    .unwrap_or(false);

                if !selected_exists {
                    self.selected_name = self.projects.first().map(|project| project.name.clone());
                }

                self.refresh_all_projects();
            }
            Err(error) => {
                self.projects.clear();
                self.snapshots.clear();
                self.selected_name = None;
                self.notice = Some(error);
            }
        }
    }

    fn refresh_all_projects(&mut self) {
        let projects = self.projects.clone();
        let mut snapshots = HashMap::new();

        for project in projects {
            let snapshot = match project_status(&project.name) {
                Ok(status) => {
                    let diagnosis = analyze_project(&status);
                    ProjectSnapshot::success(status, diagnosis)
                }
                Err(error) => ProjectSnapshot::failure(error),
            };

            snapshots.insert(project.name.clone(), snapshot);
        }

        self.snapshots = snapshots;
        self.last_refresh = Instant::now();
    }

    fn select_project(&mut self, name: String) {
        self.selected_name = Some(name);
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
                self.reload_projects();
                self.notice = Some(format!("El proyecto {name} fue eliminado de OpsDeck"));
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
                    self.refresh_all_projects();
                    self.notice = Some("Todos los proyectos fueron revisados".to_string());
                }
            });
        });

        ui.add_space(5.0);

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.auto_refresh, "Revisión automática");

            ui.add(
                egui::Slider::new(&mut self.refresh_interval_secs, 5..=300)
                    .text("intervalo en segundos"),
            );

            ui.separator();

            ui.label(format!(
                "Última revisión: hace {} s",
                self.last_refresh.elapsed().as_secs()
            ));
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

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for project in projects {
                    let is_selected = self
                        .selected_name
                        .as_ref()
                        .map(|name| name.eq_ignore_ascii_case(&project.name))
                        .unwrap_or(false);

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

                        match snapshot {
                            Some(snapshot) => {
                                if let Some(diagnosis) = snapshot.diagnosis {
                                    ui.label(format!(
                                        "{} · {}/100",
                                        diagnosis.risk, diagnosis.score
                                    ));
                                } else if snapshot.error.is_some() {
                                    ui.label("Error durante la revisión");
                                } else {
                                    ui.label("Sin diagnóstico");
                                }
                            }
                            None => {
                                ui.label("Sin revisar");
                            }
                        }

                        match &project.health_url {
                            Some(url) => {
                                ui.small(format!("Health: {url}"));
                            }
                            None => {
                                ui.small("Sin endpoint de health");
                            }
                        }
                    });

                    ui.add_space(7.0);
                }
            });

        if let Some(name) = selected_project {
            self.select_project(name);
        }

        if let Some(name) = project_to_delete {
            self.delete_target = Some(name);
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

        let Some(snapshot) = self.selected_snapshot() else {
            ui.vertical_centered(|ui| {
                ui.add_space(120.0);
                ui.heading(&selected_name);
                ui.label("Este proyecto todavía no ha sido revisado.");
            });

            return;
        };

        if let Some(error) = snapshot.error {
            ui.heading(&selected_name);
            ui.add_space(10.0);
            ui.label(egui::RichText::new("No se pudo revisar el proyecto").strong());
            ui.label(error);

            ui.add_space(10.0);

            if ui.button("Intentar nuevamente").clicked() {
                self.refresh_all_projects();
            }

            return;
        }

        let Some(status) = snapshot.status else {
            ui.label("No hay información disponible.");
            return;
        };

        let Some(diagnosis) = snapshot.diagnosis else {
            ui.label("No hay diagnóstico disponible.");
            return;
        };

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading(egui::RichText::new(&status.name).size(25.0).strong());
                ui.monospace(status.path.display().to_string());
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Abrir carpeta").clicked() {
                    self.open_selected_folder(&status);
                }

                if ui.button("Abrir en VS Code").clicked() {
                    self.open_selected_in_vscode(&status);
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

                    ui.label(egui::RichText::new("Health URL").strong());
                    ui.monospace(status.health_url.as_deref().unwrap_or("Sin endpoint"));
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
                    .max_height(240.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for finding in &diagnosis.findings {
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
                                        "Regla: {} · Penalización: -{}",
                                        finding.code, finding.penalty
                                    ));
                                },
                            );
                        }
                    });
            }
        });

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

                    if ui.button("Seleccionar").clicked() {
                        if let Some(path) = FileDialog::new()
                            .set_title("Selecciona el repositorio Git")
                            .pick_folder()
                        {
                            let fill_name = self.new_project_name.trim().is_empty();

                            self.new_project_path = path.display().to_string();

                            if fill_name {
                                if let Some(name) =
                                    path.file_name().and_then(|value| value.to_str())
                                {
                                    self.new_project_name = name.to_string();
                                }
                            }
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
                ui.label(format!("¿Quieres eliminar {project_name} de OpsDeck?"));

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
        let interval = Duration::from_secs(self.refresh_interval_secs.max(5));

        if self.auto_refresh && self.last_refresh.elapsed() >= interval {
            self.refresh_all_projects();
        }

        context.request_repaint_after(Duration::from_secs(1));
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
            .default_size(300.0)
            .min_size(240.0)
            .max_size(430.0)
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

fn status_card(ui: &mut egui::Ui, label: &str, value: usize) {
    ui.group(|ui| {
        ui.set_min_width(118.0);

        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(value.to_string()).size(24.0).strong());
            ui.label(label);
        });
    });
}
