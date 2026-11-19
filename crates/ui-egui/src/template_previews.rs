//! Bounded template thumbnails. Native layout/raster work never blocks the first frame.
use egui::{ColorImage, Context, TextureHandle};
use std::{
    collections::VecDeque,
    sync::mpsc::{Receiver, TryRecvError},
};
use wordcraft_geom::Region;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc;
const MAX_CACHED: usize = 8;
const RELEASE_AFTER: f64 = 5.0;
const WIDTH: f32 = 150.0;

#[derive(Clone, PartialEq, Eq)]
struct Key {
    template: String,
    quarters: u32,
    region: Region,
}
impl Key {
    fn new(template: &str, scale: f32, region: Region) -> Self {
        let scale = if scale.is_finite() { scale.clamp(1.0, 3.0) } else { 2.0 };
        Self { template: template.to_string(), quarters: (scale * 4.0).round() as u32, region }
    }
}

#[derive(Default)]
pub(crate) struct Previews {
    cached: VecDeque<(Key, Option<TextureHandle>)>,
    last_used: f64,
    pending: Option<(Key, Receiver<Option<ColorImage>>)>,
}
impl Previews {
    pub(crate) fn get(&mut self, ctx: &Context, template: &str, region: Region) -> Option<TextureHandle> {
        self.last_used = ctx.input(|input| input.time);
        self.poll(ctx);
        let key = Key::new(template, ctx.pixels_per_point(), region);
        if let Some(index) = self.cached.iter().position(|(k, _)| *k == key) {
            let entry = self.cached.remove(index)?;
            let texture = entry.1.clone();
            self.cached.push_back(entry);
            return texture;
        }
        if self.pending.is_some() {
            return None;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = mpsc::channel();
            self.pending = Some((key.clone(), rx));
            let repaint = ctx.clone();
            let worker_key = key.clone();
            let worker = std::thread::Builder::new().name("template-preview".into()).spawn(move || {
                let _ = tx.send(raster(worker_key));
                repaint.request_repaint();
            });
            if worker.is_err() {
                self.pending = None;
                self.insert(ctx, key, None);
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let image = raster(key.clone());
            self.insert(ctx, key, image);
        }
        None
    }
    pub(crate) fn release_idle(&mut self, ctx: &Context) {
        if self.cached.is_empty() && self.pending.is_none() {
            return;
        }
        let elapsed = (ctx.input(|input| input.time) - self.last_used).max(0.0);
        if !elapsed.is_finite() || elapsed >= RELEASE_AFTER {
            self.cached.clear();
            self.pending = None;
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(RELEASE_AFTER - elapsed));
    }
    fn poll(&mut self, ctx: &Context) {
        let Some((_, reply)) = &self.pending else {
            return;
        };
        let image = match reply.try_recv() {
            Ok(image) => image,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => None,
        };
        if let Some((key, _)) = self.pending.take() {
            self.insert(ctx, key, image);
        }
    }
    fn insert(&mut self, ctx: &Context, key: Key, image: Option<ColorImage>) {
        let texture = image.map(|image| ctx.load_texture("template preview", image, egui::TextureOptions::LINEAR));
        if self.cached.len() >= MAX_CACHED {
            self.cached.pop_front();
        }
        self.cached.push_back((key, texture));
    }
}
fn raster(key: Key) -> Option<ColorImage> {
    let mut session = wordcraft_engine::Session::new(wordcraft_doc::Document::new());
    session.system_region = key.region;
    session.run("file.new", &serde_json::json!({"template":key.template})).ok()?;
    let layout = session.export_layout();
    let page = layout.pages.first()?;
    let scale = WIDTH / page.w * key.quarters as f32 / 4.0;
    let image = wordcraft_render::render_page(&session.doc, page, scale, &crate::canvas::screen_render_options());
    Some(ColorImage::from_rgba_unmultiplied([image.width as usize, image.height as usize], &image.to_straight()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_and_failure_are_bounded() {
        let ctx = Context::default();
        ctx.set_pixels_per_point(2.0);
        let mut previews = Previews::default();
        for index in 0..32 {
            let key = Key::new(&format!("template{index}"), 2.0, Region::default());
            previews.insert(&ctx, key, Some(ColorImage::new([1, 1], vec![egui::Color32::WHITE])));
        }
        assert_eq!(previews.cached.len(), MAX_CACHED);
        let (tx, rx) = mpsc::channel();
        drop(tx);
        previews.pending = Some((Key::new("failed", ctx.pixels_per_point(), Region::default()), rx));
        previews.poll(&ctx);
        assert!(previews.pending.is_none());
        assert!(previews.get(&ctx, "failed", Region::default()).is_none());
        assert!(previews.pending.is_none(), "a failed job must not retry every frame");
    }
    #[test]
    fn idle_cache_is_released() {
        let ctx = Context::default();
        let mut previews = Previews::default();
        previews.insert(&ctx, Key::new("blank", 1.0, Region::default()), None);
        ctx.run_ui(egui::RawInput { time: Some(RELEASE_AFTER - 1.0), ..Default::default() }, |_| {}).drop_without_applying_deltas();
        previews.release_idle(&ctx);
        assert_eq!(previews.cached.len(), 1);
        ctx.run_ui(egui::RawInput { time: Some(RELEASE_AFTER), ..Default::default() }, |_| {}).drop_without_applying_deltas();
        previews.release_idle(&ctx);
        assert!(previews.cached.is_empty());
    }
}
