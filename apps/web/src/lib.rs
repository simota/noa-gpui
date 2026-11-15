//! Built twice, once with `webgpu` and once with `webgl2`. `index.html` loads the
//! WebGPU build when an adapter is available and falls back to the WebGL2 build on a
//! fresh canvas when that fails. [`start`] rejects if the GPU cannot be initialized,
//! which triggers the fallback.

#[cfg(target_arch = "wasm32")]
mod web {
    use demo::{Config, GpuContext, wgpu};
    use wasm_bindgen::prelude::*;

    #[cfg(all(feature = "webgpu", feature = "webgl2"))]
    compile_error!("build the web app with exactly one of the `webgpu` or `webgl2` features");

    #[cfg(feature = "webgpu")]
    const BACKENDS: wgpu::Backends = wgpu::Backends::BROWSER_WEBGPU;
    #[cfg(feature = "webgl2")]
    const BACKENDS: wgpu::Backends = wgpu::Backends::GL;

    /// `query` is `location.search`. `font` is a font for the characters the bundled
    /// Latin font lacks (Japanese), if the loader could fetch one: the Web has no
    /// system fonts to fall back to.
    #[wasm_bindgen]
    pub async fn start(
        canvas: web_sys::HtmlCanvasElement,
        query: String,
        font: Option<Vec<u8>>,
    ) -> Result<(), JsValue> {
        console_error_panic_hook::set_once();
        let _ = console_log::init_with_level(log::Level::Info);

        let config = Config::parse(query.trim_start_matches('?').split('&'))
            .map_err(|e| JsValue::from_str(&e))?;

        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = BACKENDS;
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|e| JsValue::from_str(&format!("create surface: {e}")))?;
        let ctx = GpuContext::new(&instance, surface, canvas.width(), canvas.height())
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let mut app = demo::app_with_gpu(config, instance, ctx, canvas);
        if let Some(font) = font {
            app = app.with_font(font);
        }
        app.spawn().map_err(|e| JsValue::from_str(&e.to_string()))
    }
}
