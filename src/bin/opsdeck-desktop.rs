use eframe::egui;
use opsdeck::{
    Project, ProjectStatus, config_path, load_config, open_in_file_manager, open_in_vscode,
    project_status,
};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OpsDeck")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([860.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "OpsDeck",
        options,
        Box::new(|creation_context| Ok(Box::new(OpsDeckApp::new(creation_context)))),
    )
}

struct OpsDeckApp {
    projects: Vec<Project>,
    selected_name: Option<String>,
    status: Option<ProjectStatus>,
    notice: Option<String>,
}

impl OpsDeckApp {
    fn new(_creation_context: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            projects: Vec::new(),
            selected_name: None,
            status: None,
            notice: None,
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

                self.refresh_selected();
            }
            Err(error) => {
                self.projects.clear();
                self.selected_name = None;
                self.status = None;
                self.notice = Some(error);
            }
        }
    }

    fn refresh_selected(&mut self) {
        let Some(name) = self.selected_name.clone() else {
            self.status = None;
            return;
        };

        match project_status(&name) {
            Ok(status) => {
                self.status = Some(status);
                self.notice = None;
            }
            Err(error) => {
                self.status = None;
                self.notice = Some(error);
            }
        }
    }

    fn select_project(&mut self, name: String) {
        self.selected_name = Some(name);
        self.refresh_selected();
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
                ui.heading(egui::RichText::new("OpsDeck").size(26.0).strong());
                ui.label("Centro de control para tus proyectos");
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Recargar proyectos").clicked() {
                    self.reload_projects();
                }

                if ui.button("Actualizar estado").clicked() {
                    self.refresh_selected();
                }
            });
        });

        ui.add_space(8.0);
    }

    fn show_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.heading("Proyectos");
        ui.separator();

        if self.projects.is_empty() {
            ui.label("No hay proyectos registrados.");
            ui.add_space(8.0);
            ui.label("Puedes registrar uno desde la CLI:");
            ui.monospace("cargo run -- add \"Nombre\" /ruta");
            return;
        }

        let projects = self.projects.clone();
        let mut selected_project = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for project in projects {
                    let is_selected = self
                        .selected_name
                        .as_ref()
                        .map(|name| name.eq_ignore_ascii_case(&project.name))
                        .unwrap_or(false);

                    ui.group(|ui| {
                        let response = ui.selectable_label(
                            is_selected,
                            egui::RichText::new(&project.name).strong(),
                        );

                        if response.clicked() {
                            selected_project = Some(project.name.clone());
                        }

                        ui.small(project.path.display().to_string());

                        match &project.health_url {
                            Some(url) => {
                                ui.small(format!("Health: {url}"));
                            }
                            None => {
                                ui.small("Sin endpoint de health");
                            }
                        }
                    });

                    ui.add_space(6.0);
                }
            });

        if let Some(name) = selected_project {
            self.select_project(name);
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
                ui.label(notice);
            });

            ui.add_space(8.0);
        }

        let Some(status) = self.status.clone() else {
            ui.vertical_centered(|ui| {
                ui.add_space(120.0);
                ui.heading("Selecciona un proyecto");
                ui.label("Aquí aparecerá su estado, rama, cambios y sincronización.");
            });

            return;
        };

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading(egui::RichText::new(&status.name).size(24.0).strong());
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
        });

        ui.add_space(10.0);
        ui.heading("Cambios locales");
        ui.separator();

        if status.raw_status.trim().is_empty() {
            ui.label("No hay cambios locales pendientes.");
        } else {
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.monospace(&status.raw_status);
                });
        }
    }
}

impl eframe::App for OpsDeckApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.style_mut().spacing.item_spacing = egui::vec2(10.0, 10.0);
        ui.style_mut().spacing.button_padding = egui::vec2(14.0, 8.0);

        egui::Panel::top("header")
            .resizable(false)
            .exact_size(76.0)
            .show(ui, |ui| {
                self.show_header(ui);
            });

        egui::Panel::left("projects")
            .resizable(true)
            .default_size(280.0)
            .min_size(220.0)
            .max_size(400.0)
            .show(ui, |ui| {
                self.show_sidebar(ui);
            });

        egui::CentralPanel::default().show(ui, |ui| {
            self.show_content(ui);
        });
    }
}

fn status_card(ui: &mut egui::Ui, label: &str, value: usize) {
    ui.group(|ui| {
        ui.set_min_width(120.0);

        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(value.to_string()).size(24.0).strong());
            ui.label(label);
        });
    });
}
