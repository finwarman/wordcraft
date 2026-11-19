//! Generate the original Classic/Pink bundle artwork from shared theme roles.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args().nth(1).ok_or("output directory required")?;
    std::fs::create_dir_all(&output)?;
    for theme in [wordcraft_ui_egui::theme::Appearance::Classic, wordcraft_ui_egui::theme::Appearance::Pink] {
        let icon = wordcraft_ui_egui::theme::application_icon(theme);
        image::save_buffer(
            std::path::Path::new(&output).join(format!("{}.png", theme.code())),
            &icon.rgba,
            icon.width,
            icon.height,
            image::ColorType::Rgba8,
        )?;
    }
    Ok(())
}
