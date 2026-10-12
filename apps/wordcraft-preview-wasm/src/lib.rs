//! Headless DOCX page renderer for a live, client-side report preview.
//!
//! Wraps the same in-memory pipeline `wordcraft-cli convert x.docx x.png` uses
//! (`wordcraft_docx::read` -> `Session::export_layout_with` -> `render_page` ->
//! `to_png`), without the CLI's path-based I/O, so it carries no UI, GPU or
//! filesystem dependency. `wordcraft-render` rasterises with `vello_cpu`
//! (software, not WebGPU), which is what keeps this out of `apps/wordcraft-web`'s
//! eframe/wgpu dependency tree.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use wordcraft_engine::Session;

/// An open document, ready to render pages on demand.
pub struct Preview {
    session: Session,
}

impl Preview {
    /// Parses a DOCX buffer. Errs on a corrupt, encrypted or unsupported document.
    pub fn open(bytes: &[u8]) -> Result<Self, String> {
        let doc = wordcraft_docx::read(bytes).map_err(|e| e.to_string())?;
        Ok(Self { session: Session::new(doc) })
    }

    pub fn page_count(&self) -> usize {
        self.session.export_layout_with(false).pages.len()
    }

    /// Renders page `page` (1-based) at `scale` px/pt as PNG bytes.
    pub fn render_page_png(&self, page: usize, scale: f32) -> Result<Vec<u8>, String> {
        let layout = self.session.export_layout_with(false);
        let index = page.checked_sub(1).ok_or("page is 1-based")?;
        let pg = layout
            .pages
            .get(index)
            .ok_or_else(|| format!("no page {page} (document has {} pages)", layout.pages.len()))?;
        let rendered = wordcraft_render::render_page(&self.session.doc, pg, scale, &Default::default());
        Ok(rendered.to_png())
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    /// JS-facing wrapper. Kept thin: all real logic lives in `Preview` above,
    /// which a native test can exercise without a wasm32 toolchain.
    #[wasm_bindgen]
    pub struct Preview(super::Preview);

    #[wasm_bindgen]
    impl Preview {
        #[wasm_bindgen(constructor)]
        pub fn new(bytes: &[u8]) -> Result<Preview, String> {
            super::Preview::open(bytes).map(Preview)
        }

        #[wasm_bindgen(js_name = pageCount)]
        pub fn page_count(&self) -> usize {
            self.0.page_count()
        }

        /// Renders page `page` (1-based) at `scale` px/pt as PNG bytes.
        #[wasm_bindgen(js_name = renderPage)]
        pub fn render_page(&self, page: usize, scale: f32) -> Result<Vec<u8>, String> {
            self.0.render_page_png(page, scale)
        }
    }

    #[wasm_bindgen(start)]
    pub fn init() {
        console_error_panic_hook::set_once();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_garbage_bytes() {
        assert!(Preview::open(b"not a docx").is_err());
    }
}
