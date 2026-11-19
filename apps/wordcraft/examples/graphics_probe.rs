//! Non-activating synthetic Metal/texture probe. No user preferences or documents.
#[cfg(target_os = "macos")]
use objc2_metal::MTLDevice;
use std::time::{Duration, Instant};
use wordcraft_ui_egui::{Services, WordApp};

struct Probe {
    app: WordApp,
    started: Instant,
    latency: u32,
    scene: String,
    frames: u32,
    previous: Option<Instant>,
    intervals: Vec<f64>,
    busy: bool,
}

impl eframe::App for Probe {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.logic(ctx);
        if self.started.elapsed() < Duration::from_secs(3) {
            if self.busy {
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
            return;
        }
        let manager = ctx.tex_manager();
        let manager = manager.read();
        let textures: Vec<_> = manager
            .allocated()
            .map(|(_, texture)| serde_json::json!({"name":texture.name, "size":texture.size, "bytes":texture.bytes_used()}))
            .collect();
        let gpu = frame.wgpu_render_state().map(|state| {
            let info = state.adapter.get_info();
            serde_json::json!({"adapter":info.name, "backend":format!("{:?}", info.backend),
                "counters":format!("{:?}", state.device.get_internal_counters())})
        });
        #[cfg(target_os = "macos")]
        let metal = objc2_metal::MTLCreateSystemDefaultDevice().map(|device| device.currentAllocatedSize());
        #[cfg(not(target_os = "macos"))]
        let metal: Option<usize> = None;
        self.intervals.sort_by(f64::total_cmp);
        let p50 = self.intervals.get(self.intervals.len() / 2).copied();
        let p95 = self.intervals.get(self.intervals.len() * 95 / 100).copied();
        println!(
            "{}",
            serde_json::json!({"pid":std::process::id(), "latency":self.latency,
            "scene":self.scene, "frames":self.frames, "busy":self.busy, "uiCallbackP50Ms":p50, "uiCallbackP95Ms":p95, "textures":textures,
            "gpu":gpu, "defaultMetalAllocatedBytes":metal})
        );
        // Keep this owned, synthetic process alive briefly for an external vmmap snapshot.
        std::thread::sleep(Duration::from_secs(2));
        std::process::exit(0);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        if let Some(previous) = self.previous
            && self.intervals.len() < 4096
        {
            self.intervals.push(now.duration_since(previous).as_secs_f64() * 1000.0);
        }
        self.previous = Some(now);
        self.frames = self.frames.saturating_add(1);
        self.app.ui(ui);
    }
}

fn main() -> eframe::Result {
    let scene = std::env::args().nth(1).unwrap_or_else(|| "sample".into());
    let latency = std::env::args().nth(2).and_then(|value| value.parse::<u32>().ok()).unwrap_or(2).clamp(1, 2);
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1440.0, 920.0]).with_visible(false).with_active(false),
        ..Default::default()
    };
    options.wgpu_options.surface.desired_maximum_frame_latency = Some(latency);
    #[cfg(target_os = "macos")]
    {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
            builder.with_activation_policy(ActivationPolicy::Accessory).with_activate_ignoring_other_apps(false).with_default_menu(false);
        }));
    }
    let busy = std::env::args().nth(3).as_deref() == Some("--busy");
    let started = Instant::now();
    eframe::run_native(
        "Fin graphics probe",
        options,
        Box::new(move |_| {
            let doc = if scene == "sample" { wordcraft_engine::sample::sample_document() } else { wordcraft_doc::Document::new() };
            let mut app = WordApp::new(wordcraft_engine::Session::new(doc), Services::default());
            if scene == "start" {
                app.start_screen();
            }
            Ok(Box::new(Probe { app, started, latency, scene, frames: 0, previous: None, intervals: Vec::new(), busy }))
        }),
    )
}
