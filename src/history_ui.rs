use crate::history::ReviewRecord;
use eframe::egui;

pub fn show_history_panel(ui: &mut egui::Ui, reviews: &[ReviewRecord]) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading("Historial");
                ui.label("Evolución de las revisiones locales");
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("{} registros", reviews.len()));
            });
        });

        ui.add_space(8.0);

        if reviews.is_empty() {
            ui.label("Todavía no hay revisiones guardadas para este proyecto.");
            return;
        }

        let ordered = reviews.iter().rev().collect::<Vec<_>>();

        let score_values = ordered
            .iter()
            .map(|record| record.score as f32)
            .collect::<Vec<_>>();

        let latency_values = ordered
            .iter()
            .filter_map(|record| record.latency_ms.map(|value| value as f32))
            .collect::<Vec<_>>();

        let latest = ordered.last().expect("Debe existir al menos una revisión");

        ui.horizontal_wrapped(|ui| {
            history_stat(ui, "Puntuación actual", format!("{}/100", latest.score));
            history_stat(ui, "Riesgo", latest.risk.clone());
            history_stat(ui, "Health", latest.health_state.clone());

            history_stat(
                ui,
                "Latencia actual",
                latest
                    .latency_ms
                    .map(|value| format!("{value} ms"))
                    .unwrap_or_else(|| "Sin datos".to_string()),
            );

            history_stat(ui, "Cambios locales", latest.changes_total.to_string());
        });

        ui.add_space(12.0);

        ui.label(egui::RichText::new("Puntuación").strong());

        draw_series_chart(ui, &score_values, 0.0, 100.0, "");

        ui.add_space(12.0);

        ui.label(egui::RichText::new("Latencia HTTP").strong());

        if latency_values.is_empty() {
            ui.label("Este proyecto no tiene mediciones HTTP disponibles.");
        } else {
            let maximum_latency = latency_values
                .iter()
                .copied()
                .fold(0.0_f32, f32::max)
                .max(100.0);

            draw_series_chart(ui, &latency_values, 0.0, maximum_latency, " ms");
        }

        ui.add_space(10.0);

        egui::ScrollArea::vertical()
            .max_height(180.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("history_records")
                    .striped(true)
                    .num_columns(6)
                    .spacing([18.0, 7.0])
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Revisión").strong());
                        ui.label(egui::RichText::new("Puntuación").strong());
                        ui.label(egui::RichText::new("Riesgo").strong());
                        ui.label(egui::RichText::new("Health").strong());
                        ui.label(egui::RichText::new("Latencia").strong());
                        ui.label(egui::RichText::new("Cambios").strong());
                        ui.end_row();

                        for (index, record) in ordered.iter().rev().take(15).enumerate() {
                            ui.label(format!("#{}", reviews.len() - index));
                            ui.label(format!("{}/100", record.score));
                            ui.label(&record.risk);
                            ui.label(&record.health_state);

                            ui.label(
                                record
                                    .latency_ms
                                    .map(|value| format!("{value} ms"))
                                    .unwrap_or_else(|| "—".to_string()),
                            );

                            ui.label(record.changes_total.to_string());
                            ui.end_row();
                        }
                    });
            });
    });
}

fn history_stat(ui: &mut egui::Ui, label: &str, value: String) {
    ui.group(|ui| {
        ui.set_min_width(145.0);

        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(value).size(18.0).strong());

            ui.small(label);
        });
    });
}

fn draw_series_chart(ui: &mut egui::Ui, values: &[f32], minimum: f32, maximum: f32, suffix: &str) {
    if values.is_empty() {
        ui.label("No hay datos suficientes.");
        return;
    }

    let width = ui.available_width().max(260.0);
    let desired_size = egui::vec2(width, 170.0);

    let (rect, _response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

    let painter = ui.painter_at(rect);
    let plot_rect = rect.shrink2(egui::vec2(34.0, 22.0));

    let grid_stroke = egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);

    let series_stroke = egui::Stroke::new(2.0, ui.visuals().selection.stroke.color);

    let text_color = ui.visuals().text_color();

    for step in 0..=4 {
        let ratio = step as f32 / 4.0;
        let y = plot_rect.bottom() - ratio * plot_rect.height();

        painter.line_segment(
            [
                egui::pos2(plot_rect.left(), y),
                egui::pos2(plot_rect.right(), y),
            ],
            grid_stroke,
        );

        let value = minimum + ratio * (maximum - minimum);

        painter.text(
            egui::pos2(plot_rect.left() - 6.0, y),
            egui::Align2::RIGHT_CENTER,
            format!("{value:.0}"),
            egui::FontId::monospace(10.0),
            text_color,
        );
    }

    painter.line_segment(
        [
            egui::pos2(plot_rect.left(), plot_rect.bottom()),
            egui::pos2(plot_rect.right(), plot_rect.bottom()),
        ],
        grid_stroke,
    );

    let range = (maximum - minimum).max(1.0);

    let points = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let x_ratio = if values.len() <= 1 {
                0.5
            } else {
                index as f32 / (values.len() - 1) as f32
            };

            let normalized = ((*value - minimum) / range).clamp(0.0, 1.0);

            let x = plot_rect.left() + x_ratio * plot_rect.width();

            let y = plot_rect.bottom() - normalized * plot_rect.height();

            egui::pos2(x, y)
        })
        .collect::<Vec<_>>();

    for pair in points.windows(2) {
        painter.line_segment([pair[0], pair[1]], series_stroke);
    }

    for point in &points {
        painter.circle_filled(*point, 3.0, series_stroke.color);
    }

    if let Some(latest) = values.last() {
        painter.text(
            egui::pos2(plot_rect.right(), plot_rect.top() - 8.0),
            egui::Align2::RIGHT_BOTTOM,
            format!("Actual: {latest:.0}{suffix}"),
            egui::FontId::monospace(11.0),
            text_color,
        );
    }

    painter.text(
        egui::pos2(plot_rect.left(), plot_rect.bottom() + 8.0),
        egui::Align2::LEFT_TOP,
        "Más antiguo",
        egui::FontId::monospace(10.0),
        text_color,
    );

    painter.text(
        egui::pos2(plot_rect.right(), plot_rect.bottom() + 8.0),
        egui::Align2::RIGHT_TOP,
        "Más reciente",
        egui::FontId::monospace(10.0),
        text_color,
    );
}
