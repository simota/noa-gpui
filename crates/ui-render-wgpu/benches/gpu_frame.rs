//! Renders a 2560x1600 (1280x800 @2x) frame of N widgets (rounded quad + image + text
//! each) offscreen and measures, per frame:
//!   - prepare: batching, instance upload and text preparation (CPU),
//!   - encode:  recording and submitting the render pass (CPU),
//!   - gpu:     from submit until the GPU has finished the frame.
//!
//! ```sh
//! cargo bench -p noa-gpui-render-wgpu --features native --bench gpu_frame
//! BENCH_WIDGETS=5000 cargo bench -p noa-gpui-render-wgpu --features native --bench gpu_frame
//! ```

use std::time::{Duration, Instant};

use noa_gpui_render_wgpu::{Renderer, Viewport, wgpu};
use noa_gpui_scene::{Border, Color, DisplayList, Rect};
use noa_gpui_text::{FontOptions, TextStore, TextStyle};

const WIDTH: u32 = 2560;
const HEIGHT: u32 = 1600;
const SCALE: f32 = 2.0;
const FRAMES: usize = 300;

fn percentiles(name: &str, samples: &mut [Duration]) {
    samples.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let at = |q: f64| ms(samples[((samples.len() - 1) as f64 * q).round() as usize]);
    println!(
        "{name:<8} p50 {:.3} / p95 {:.3} / p99 {:.3} / max {:.3} ms",
        at(0.50),
        at(0.95),
        at(0.99),
        ms(samples[samples.len() - 1])
    );
}

fn main() {
    let widgets: u32 = std::env::var("BENCH_WIDGETS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1500);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&Default::default())).expect("adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: noa_gpui_render_wgpu::required_limits(&adapter),
        ..Default::default()
    }))
    .expect("device");
    let info = adapter.get_info();
    println!(
        "{} ({:?}), {WIDTH}x{HEIGHT}, {widgets} widgets",
        info.name, info.backend
    );

    let format = wgpu::TextureFormat::Bgra8UnormSrgb;
    let mut renderer = Renderer::new(&device, &queue, format);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let icons: Vec<_> = (0..4u8)
        .map(|i| renderer.create_image(&device, &queue, 2, 2, &[60 * i, 120, 200, 255].repeat(4)))
        .collect();

    let mut text = TextStore::new(FontOptions {
        locale: "en-US".into(),
        load_system_fonts: false,
    });
    let logical = (WIDTH as f32 / SCALE, HEIGHT as f32 / SCALE);
    let cols = ((widgets as f32 * logical.0 / logical.1).sqrt().ceil() as u32).max(1);
    let cell = (logical.0 / cols as f32).min(logical.1 / widgets.div_ceil(cols) as f32);
    let labels: Vec<_> = (0..widgets)
        .map(|i| {
            let id = text.create(
                &i.to_string(),
                TextStyle::new((cell * 0.3).clamp(3.0, 14.0)),
                None,
            );
            (id, text.size(id))
        })
        .collect();
    let build = |list: &mut DisplayList, frame: usize| {
        list.clear();
        for i in 0..widgets {
            let (x, y) = ((i % cols) as f32 * cell, (i / cols) as f32 * cell);
            let shade = ((i as usize + frame) % 64) as f32 / 255.0;
            list.rounded_rect(
                Rect::new(x, y, cell - 2.0, cell - 2.0),
                cell * 0.18,
                Color::rgba(0.7 + shade, 0.8, 0.9, 1.0),
                Border {
                    width: 1.0,
                    color: Color::hex(0x445566),
                },
            );
            list.image(
                Rect::new(x + 2.0, y + 2.0, cell * 0.35, cell * 0.35),
                icons[(i % 4) as usize],
            );
            let (id, size) = labels[i as usize];
            list.text(
                Rect::new(x + 2.0, y + cell * 0.5, size.width, size.height),
                id,
                Color::BLACK,
            );
        }
    };

    let viewport = Viewport {
        width: WIDTH,
        height: HEIGHT,
        scale_factor: SCALE,
    };
    let mut list = DisplayList::new();
    let (mut prepare, mut encode, mut gpu) = (Vec::new(), Vec::new(), Vec::new());
    for frame in 0..FRAMES + 10 {
        build(&mut list, frame); // a changed list every frame: no cache hits
        let t0 = Instant::now();
        renderer
            .prepare(&device, &queue, &list, &mut text, viewport)
            .expect("prepare");
        let t1 = Instant::now();
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer
            .encode(&mut encoder, &view, Color::WHITE)
            .expect("encode");
        queue.submit([encoder.finish()]);
        let t2 = Instant::now();
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        let t3 = Instant::now();
        renderer.end_frame();
        if frame >= 10 {
            prepare.push(t1 - t0);
            encode.push(t2 - t1);
            gpu.push(t3 - t2);
        }
    }
    let stats = renderer.stats();
    println!(
        "{} commands -> {} batches ({} quads, {} images, {} texts)",
        list.len(),
        stats.batches,
        stats.quads,
        stats.images,
        stats.texts
    );
    percentiles("prepare", &mut prepare);
    percentiles("encode", &mut encode);
    percentiles("gpu", &mut gpu);

    let mut cached = Vec::new();
    for _ in 0..FRAMES {
        let t0 = Instant::now();
        renderer
            .prepare(&device, &queue, &list, &mut text, viewport)
            .expect("prepare");
        cached.push(t0.elapsed());
    }
    assert!(renderer.stats().cached);
    percentiles("cached", &mut cached);
}
