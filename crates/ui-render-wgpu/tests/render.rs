use std::sync::{Mutex, MutexGuard};

use noa_gpui_render_wgpu::{Renderer, Viewport, wgpu};
use noa_gpui_scene::{Border, Color, DisplayList, Rect};
use noa_gpui_text::{FontOptions, TextSpan, TextStore, TextStyle};

const SIZE: u32 = 64;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const WHITE: [u8; 4] = [255, 255, 255, 255];

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    // Declared last so it is dropped last: the next test starts after this device is gone.
    _turn: MutexGuard<'static, ()>,
}

/// One device at a time. CI renders on lavapipe (software Vulkan), which crashed with a
/// segmentation fault while several tests created and dropped devices in parallel.
fn turn() -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    // A test that panics while holding the lock must not fail the tests after it.
    TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn gpu() -> Gpu {
    let turn = turn();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("a GPU adapter is required for render tests");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: noa_gpui_render_wgpu::required_limits(&adapter),
        ..Default::default()
    }))
    .expect("device");
    Gpu {
        device,
        queue,
        _turn: turn,
    }
}

fn render(gpu: &Gpu, renderer: &mut Renderer, list: &DisplayList, text: &mut TextStore) -> Vec<u8> {
    let Gpu { device, queue, .. } = gpu;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let viewport = Viewport {
        width: SIZE,
        height: SIZE,
        scale_factor: 1.0,
    };
    renderer
        .prepare(device, queue, list, text, viewport)
        .expect("prepare");

    let bytes_per_row = SIZE * 4; // 256: already aligned for copies.
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (bytes_per_row * SIZE) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer
        .encode(&mut encoder, &view, Color::WHITE)
        .expect("render");
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(SIZE),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    renderer.end_frame();

    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.expect("map"));
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll");
    readback
        .slice(..)
        .get_mapped_range()
        .expect("mapped")
        .to_vec()
}

fn pixel(data: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * SIZE + x) * 4) as usize;
    data[i..i + 4].try_into().unwrap()
}

fn close(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 2)
}

fn text_store() -> TextStore {
    TextStore::new(FontOptions {
        locale: "en-US".into(),
        load_system_fonts: false,
    })
}

/// A rich layout's spans keep their own color, the rest takes the command's.
#[test]
fn draws_span_colors_in_one_text_layout() {
    let gpu = gpu();
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
    let mut text = text_store();
    let style = TextStyle::new(24.0).weight(900);
    let spans = [TextSpan::new(0..2).color(Color::hex(0xff0000))];
    let layout = text.create_rich("HHHH", &spans, style, None);
    let split = text.caret_x(layout, 2);
    let size = text.size(layout);
    let mut list = DisplayList::new();
    list.text(
        Rect::from_origin_size(noa_gpui_scene::Point::new(2.0, 2.0), size),
        layout,
        Color::hex(0x0000ff),
    );
    let data = render(&gpu, &mut renderer, &list, &mut text);
    let count = |range: std::ops::Range<u32>, wanted: fn([u8; 4]) -> bool| {
        (2..2 + size.height as u32)
            .flat_map(|y| range.clone().map(move |x| (x, y)))
            .filter(|&(x, y)| wanted(pixel(&data, x, y)))
            .count()
    };
    let left = 2..2 + split as u32;
    // The text runs past the 64 px target: count only what is on it.
    let right = 2 + split as u32 + 1..(2 + size.width as u32).min(SIZE);
    let red = |p: [u8; 4]| p[0] > 200 && p[2] < 80;
    let blue = |p: [u8; 4]| p[2] > 200 && p[0] < 80;
    assert!(count(left.clone(), red) > 20, "the span is red");
    assert_eq!(count(left, blue), 0);
    assert!(count(right.clone(), blue) > 20, "the rest is blue");
    assert_eq!(count(right, red), 0);
}

#[test]
fn draws_quads_images_text_and_clips() {
    let gpu = gpu();
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
    let mut text = text_store();

    let magenta = [255u8, 0, 255, 255];
    let image = renderer.create_image(&gpu.device, &gpu.queue, 2, 2, &magenta.repeat(4));
    let label = text.create("Hi", TextStyle::new(16.0).weight(700), None);
    let label_size = text.size(label);

    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(0.0, 0.0, 20.0, 20.0), Color::hex(0xff0000));
    list.rounded_rect(
        Rect::new(30.0, 0.0, 20.0, 20.0),
        10.0,
        Color::hex(0x0000ff),
        Border {
            width: 3.0,
            color: Color::hex(0x00ff00),
        },
    );
    list.push_clip(Rect::new(0.0, 24.0, 10.0, 10.0));
    list.fill_rect(Rect::new(0.0, 24.0, 30.0, 10.0), Color::hex(0x00ff00));
    list.pop_clip();
    list.image(Rect::new(40.0, 24.0, 10.0, 10.0), image);
    list.text(
        Rect::from_origin_size(noa_gpui_scene::Point::new(4.0, 40.0), label_size),
        label,
        Color::BLACK,
    );

    let data = render(&gpu, &mut renderer, &list, &mut text);

    assert!(close(pixel(&data, 10, 10), [255, 0, 0, 255]), "quad");
    assert!(
        close(pixel(&data, 40, 10), [0, 0, 255, 255]),
        "rounded fill"
    );
    assert!(close(pixel(&data, 40, 1), [0, 255, 0, 255]), "border");
    assert!(
        close(pixel(&data, 30, 0), WHITE),
        "rounded corner is transparent"
    );
    assert!(close(pixel(&data, 5, 29), [0, 255, 0, 255]), "inside clip");
    assert!(close(pixel(&data, 20, 29), WHITE), "outside clip");
    assert!(close(pixel(&data, 45, 29), magenta), "image");

    let dark = (40..40 + label_size.height as u32)
        .flat_map(|y| (4..4 + label_size.width as u32).map(move |x| (x, y)))
        .filter(|&(x, y)| pixel(&data, x, y)[0] < 100)
        .count();
    assert!(dark > 20, "text produced only {dark} dark pixels");

    let stats = renderer.stats();
    assert_eq!((stats.quads, stats.images, stats.texts), (3, 1, 1));
}

#[test]
fn later_commands_paint_over_earlier_ones() {
    let gpu = gpu();
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
    let mut text = text_store();
    let label = text.create("MM", TextStyle::new(20.0).weight(900), None);
    let label_rect = Rect::from_origin_size(noa_gpui_scene::Point::new(0.0, 0.0), text.size(label));

    let mut list = DisplayList::new();
    list.text(label_rect, label, Color::BLACK);
    list.fill_rect(Rect::new(0.0, 0.0, 64.0, 64.0), Color::hex(0x00ff00));
    let data = render(&gpu, &mut renderer, &list, &mut text);
    assert!(
        (0..SIZE).all(|y| (0..SIZE).all(|x| close(pixel(&data, x, y), [0, 255, 0, 255]))),
        "quad must cover the text drawn before it"
    );
    assert_eq!(renderer.stats().batches, 2);
}

#[test]
fn many_widgets_render_in_few_batches() {
    let gpu = gpu();
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
    let mut text = text_store();
    let image = renderer.create_image(&gpu.device, &gpu.queue, 1, 1, &[0, 0, 0, 255]);
    let labels: Vec<_> = (0..10)
        .map(|i| text.create(&i.to_string(), TextStyle::new(4.0), None))
        .collect();

    let mut list = DisplayList::new();
    for i in 0..1024u32 {
        let (x, y) = ((i % 32) as f32 * 2.0, (i / 32) as f32 * 2.0);
        list.fill_rect(Rect::new(x, y, 2.0, 2.0), Color::hex(0xcccccc));
        list.image(Rect::new(x, y, 1.0, 1.0), image);
        let label = labels[i as usize % labels.len()];
        list.text(Rect::new(x + 1.0, y, 1.0, 2.0), label, Color::BLACK);
    }
    render(&gpu, &mut renderer, &list, &mut text);
    let stats = renderer.stats();
    assert_eq!((stats.quads, stats.images, stats.texts), (1024, 1024, 1024));
    assert_eq!(stats.batches, 3);
}

fn grid(list: &mut DisplayList, count: u32, color: u32) {
    list.clear();
    for i in 0..count {
        let (x, y) = ((i % 64) as f32, (i / 64 % 64) as f32);
        list.fill_rect(Rect::new(x, y, 1.0, 1.0), Color::hex(color));
    }
}

#[test]
fn unchanged_frames_reuse_the_prepared_frame() {
    let gpu = gpu();
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
    let mut text = text_store();
    let label = text.create("Hi", TextStyle::new(16.0), None);
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(0.0, 0.0, 32.0, 32.0), Color::hex(0x336699));
    list.text(Rect::new(0.0, 32.0, 30.0, 20.0), label, Color::BLACK);

    let first = render(&gpu, &mut renderer, &list, &mut text);
    assert!(!renderer.stats().cached);
    let second = render(&gpu, &mut renderer, &list, &mut text);
    assert!(renderer.stats().cached);
    assert_eq!(first, second, "a cached frame draws the same pixels");

    list.fill_rect(Rect::new(40.0, 0.0, 8.0, 8.0), Color::BLACK);
    render(&gpu, &mut renderer, &list, &mut text);
    assert!(!renderer.stats().cached);
    text.create("other", TextStyle::new(12.0), None);
    render(&gpu, &mut renderer, &list, &mut text);
    assert!(!renderer.stats().cached);
}

#[test]
fn switching_text_stores_invalidates_the_prepared_frame() {
    let gpu = gpu();
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
    let mut first = text_store();
    let mut second = text_store();
    let style = TextStyle::new(20.0);
    let a = first.create("Hi", style, None);
    let b = second.create("WW", style, None);
    assert_eq!(a, b, "both stores allocate the same local layout ID");
    let mut list = DisplayList::new();
    list.text(Rect::new(0.0, 0.0, 64.0, 30.0), a, Color::BLACK);

    let before = render(&gpu, &mut renderer, &list, &mut first);
    let after = render(&gpu, &mut renderer, &list, &mut second);
    assert!(!renderer.stats().cached, "the text store changed");
    assert_ne!(before, after, "the new store's text is drawn");
    assert_eq!(after, render(&gpu, &mut renderer, &list, &mut second));
    assert!(
        renderer.stats().cached,
        "an unchanged store still reuses the frame"
    );
}

#[test]
fn steady_state_frames_allocate_no_gpu_buffers() {
    let gpu = gpu();
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
    let mut text = text_store();
    let mut list = DisplayList::new();

    grid(&mut list, 10, 0xff0000);
    render(&gpu, &mut renderer, &list, &mut text);
    assert_eq!(
        renderer.stats().buffer_allocations,
        1,
        "first upload allocates"
    );

    grid(&mut list, 4000, 0x00ff00);
    render(&gpu, &mut renderer, &list, &mut text);
    assert_eq!(
        renderer.stats().buffer_allocations,
        1,
        "growing past capacity reallocates once"
    );

    for (count, color) in [(4000, 0x0000ff), (100, 0x00ffff), (3000, 0xff00ff)] {
        grid(&mut list, count, color);
        render(&gpu, &mut renderer, &list, &mut text);
        let stats = renderer.stats();
        assert!(!stats.cached);
        assert_eq!(
            stats.buffer_allocations, 0,
            "{count} quads fit in the existing buffer"
        );
    }
}

#[test]
fn text_still_renders_after_trimming_caches() {
    let gpu = gpu();
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
    let mut text = text_store();
    let label = text.create("Hi", TextStyle::new(20.0).weight(800), None);
    let size = text.size(label);
    let mut list = DisplayList::new();
    list.text(
        Rect::from_origin_size(noa_gpui_scene::Point::new(4.0, 4.0), size),
        label,
        Color::BLACK,
    );

    let before = render(&gpu, &mut renderer, &list, &mut text);
    renderer.trim_caches(&gpu.device, &gpu.queue);
    let after = render(&gpu, &mut renderer, &list, &mut text);
    assert!(
        !renderer.stats().cached,
        "trimming drops the prepared frame"
    );
    let dark = |data: &[u8]| {
        (0..SIZE * SIZE)
            .filter(|i| data[*i as usize * 4] < 100)
            .count()
    };
    assert!(dark(&after) > 20);
    assert_eq!(dark(&before), dark(&after));
}

#[test]
fn glyph_atlas_growth_is_bounded_by_the_texture_limit() {
    // A device capped at 512 px textures: the glyph atlas may grow 256 -> 512, no further.
    let turn = turn();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let options = noa_gpui_render_wgpu::GpuOptions {
        max_texture_size: Some(512),
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: noa_gpui_render_wgpu::limits_with(&adapter, options),
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(device.limits().max_texture_dimension_2d, 512);
    let gpu = Gpu {
        device,
        queue,
        _turn: turn,
    };
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
    let mut text = text_store();

    // Far more large glyphs than a 512x512 atlas holds, all needed in one frame.
    let alphabet: String = ('A'..='Z').chain('a'..='z').chain('0'..='9').collect();
    let mut list = DisplayList::new();
    for size in [60.0, 72.0, 84.0, 96.0] {
        let id = text.create(&alphabet, TextStyle::new(size), None);
        list.text(Rect::new(0.0, 0.0, 64.0, 64.0), id, Color::BLACK);
    }
    let viewport = Viewport {
        width: SIZE,
        height: SIZE,
        scale_factor: 4.0,
    };
    let result = renderer.prepare(&gpu.device, &gpu.queue, &list, &mut text, viewport);
    assert!(
        matches!(
            result,
            Err(noa_gpui_render_wgpu::RenderError::TextPrepare(_))
        ),
        "the frame reports a full atlas instead of exceeding the limit"
    );

    let small = text.create("ok", TextStyle::new(16.0).weight(800), None);
    let mut list = DisplayList::new();
    list.text(Rect::new(4.0, 4.0, 40.0, 24.0), small, Color::BLACK);
    let data = render(&gpu, &mut renderer, &list, &mut text);
    assert!(
        (0..SIZE * SIZE)
            .filter(|i| data[*i as usize * 4] < 100)
            .count()
            > 20
    );
}

#[test]
fn draws_round_capped_lines_and_triangles() {
    use noa_gpui_scene::{Direction, DrawCommand, Point, Shape};
    let gpu = gpu();
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
    let mut text = text_store();
    let mut list = DisplayList::new();
    let red = Color::rgba(1.0, 0.0, 0.0, 1.0);
    let blue = Color::rgba(0.0, 0.0, 1.0, 1.0);
    list.push(DrawCommand::Shape {
        shape: Shape::Line {
            from: Point::new(4.0, 4.0),
            to: Point::new(28.0, 28.0),
            width: 4.0,
        },
        color: red,
    });
    list.push(DrawCommand::Shape {
        shape: Shape::Triangle {
            rect: Rect::new(36.0, 36.0, 24.0, 24.0),
            direction: Direction::Down,
        },
        color: blue,
    });
    let data = render(&gpu, &mut renderer, &list, &mut text);
    let (r, b) = ([255, 0, 0, 255], [0, 0, 255, 255]);
    assert!(close(pixel(&data, 16, 16), r), "on the line");
    assert!(close(pixel(&data, 5, 5), r), "round cap");
    assert!(close(pixel(&data, 24, 8), WHITE), "off the line");
    assert!(close(pixel(&data, 48, 40), b), "wide top of the triangle");
    assert!(close(pixel(&data, 48, 57), b), "near the apex");
    assert!(
        close(pixel(&data, 38, 57), WHITE),
        "outside the sloped side"
    );
}

#[test]
fn draws_linear_gradients_interpolated_in_srgb() {
    use noa_gpui_scene::{DrawCommand, Shape};
    let gpu = gpu();
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
    let mut text = text_store();
    let mut list = DisplayList::new();
    list.push(DrawCommand::Shape {
        shape: Shape::Gradient {
            rect: Rect::new(0.0, 0.0, 64.0, 16.0),
            radius: 0.0,
            end: Color::WHITE,
            vertical: false,
        },
        color: Color::BLACK,
    });
    list.push(DrawCommand::Shape {
        shape: Shape::Gradient {
            rect: Rect::new(16.0, 24.0, 32.0, 40.0),
            radius: 8.0,
            end: Color::rgba(0.0, 0.0, 1.0, 1.0),
            vertical: true,
        },
        color: Color::rgba(1.0, 0.0, 0.0, 1.0),
    });
    let data = render(&gpu, &mut renderer, &list, &mut text);
    let near = |a: [u8; 4], b: [u8; 4]| a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 4);
    assert!(
        near(pixel(&data, 0, 8), [2, 2, 2, 255]),
        "{:?}",
        pixel(&data, 0, 8)
    );
    // Halfway in sRGB is 128, not the 188 of a linear-light blend.
    assert!(
        near(pixel(&data, 32, 8), [130, 130, 130, 255]),
        "{:?}",
        pixel(&data, 32, 8)
    );
    assert!(
        near(pixel(&data, 63, 8), [253, 253, 253, 255]),
        "{:?}",
        pixel(&data, 63, 8)
    );
    assert!(pixel(&data, 32, 26)[0] > 230, "red at the top");
    assert!(pixel(&data, 32, 62)[2] > 230, "blue at the bottom");
    assert_eq!(pixel(&data, 16, 24), WHITE, "rounded corner left clear");
}

#[test]
fn draws_stars_and_arcs() {
    use noa_gpui_scene::{DrawCommand, Point, Shape};
    let gpu = gpu();
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
    let mut text = text_store();
    let mut list = DisplayList::new();
    let red = Color::rgba(1.0, 0.0, 0.0, 1.0);
    let blue = Color::rgba(0.0, 0.0, 1.0, 1.0);
    list.push(DrawCommand::Shape {
        shape: Shape::Star {
            rect: Rect::new(0.0, 0.0, 32.0, 32.0),
            outline: 0.0,
        },
        color: red,
    });
    list.push(DrawCommand::Shape {
        shape: Shape::Star {
            rect: Rect::new(32.0, 0.0, 32.0, 32.0),
            outline: 2.0,
        },
        color: red,
    });
    list.push(DrawCommand::Shape {
        shape: Shape::Arc {
            center: Point::new(32.0, 48.0),
            radius: 12.0,
            width: 4.0,
            sweep: 0.25,
        },
        color: blue,
    });
    let data = render(&gpu, &mut renderer, &list, &mut text);
    let (r, b) = ([255, 0, 0, 255], [0, 0, 255, 255]);
    assert!(close(pixel(&data, 16, 17), r), "star center");
    assert!(close(pixel(&data, 16, 6), r), "top point");
    assert!(close(pixel(&data, 2, 2), WHITE), "outside the star");
    assert!(
        close(pixel(&data, 16, 30), WHITE),
        "between the lower points"
    );
    assert!(close(pixel(&data, 48, 17), WHITE), "hollow outlined star");
    assert!(close(pixel(&data, 48, 5), r), "outline at the top point");
    // On the arc at 1:30 (45 degrees), not at 6 or 9 o'clock.
    let d = 12.0 * std::f32::consts::FRAC_1_SQRT_2;
    assert!(
        close(pixel(&data, (32.0 + d) as u32, (48.0 - d) as u32), b),
        "arc"
    );
    assert!(
        close(pixel(&data, 32, 60), WHITE),
        "6 o'clock is not on the arc"
    );
    assert!(
        close(pixel(&data, 20, 48), WHITE),
        "9 o'clock is not on the arc"
    );
}

#[test]
fn backdrops_blur_what_is_below_them_only() {
    use noa_gpui_scene::DrawCommand;
    let gpu = gpu();
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
    let mut text = text_store();
    let mut list = DisplayList::new();
    let red = Color::rgba(1.0, 0.0, 0.0, 1.0);
    let blue = Color::rgba(0.0, 0.0, 1.0, 1.0);
    let green = Color::rgba(0.0, 1.0, 0.0, 1.0);
    list.fill_rect(Rect::new(0.0, 0.0, 32.0, 64.0), red);
    list.fill_rect(Rect::new(32.0, 0.0, 32.0, 64.0), blue);
    list.push(DrawCommand::Backdrop {
        rect: Rect::new(8.0, 16.0, 40.0, 32.0),
        radius: 0.0,
        blur: 16.0,
        tint: Color::TRANSPARENT,
    });
    // Drawn after the backdrop, these stay sharp on top even though the quad batch
    // before the backdrop could have taken them. The outside one comes first, so that
    // batch is the only one it could join.
    list.fill_rect(Rect::new(49.0, 16.0, 15.0, 32.0), green);
    list.fill_rect(Rect::new(26.0, 30.0, 4.0, 4.0), green);
    let data = render(&gpu, &mut renderer, &list, &mut text);
    let p = |x, y| pixel(&data, x, y);
    assert!(close(p(31, 4), [255, 0, 0, 255]) && close(p(32, 4), [0, 0, 255, 255]));
    let (left, right) = (p(30, 20), p(34, 20));
    assert!(left[2] > 30 && left[0] > 100, "{left:?}");
    assert!(right[0] > 30 && right[2] > 100, "{right:?}");
    assert!(p(10, 40)[0] > 230 && p(10, 40)[2] < 25, "{:?}", p(10, 40));
    assert!(close(p(27, 31), [0, 255, 0, 255]), "{:?}", p(27, 31));
    assert!(
        p(46, 32)[1] < 8,
        "green leaked into the blur: {:?}",
        p(46, 32)
    );
    assert!(close(p(56, 32), [0, 255, 0, 255]));
}

#[test]
fn backdrop_tints_and_rounds_its_corners() {
    use noa_gpui_scene::DrawCommand;
    let gpu = gpu();
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
    let mut text = text_store();
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(0.0, 0.0, 64.0, 64.0), Color::BLACK);
    list.push(DrawCommand::Backdrop {
        rect: Rect::new(0.0, 0.0, 64.0, 64.0),
        radius: 16.0,
        blur: 4.0,
        tint: Color::WHITE.with_alpha(0.5),
    });
    let data = render(&gpu, &mut renderer, &list, &mut text);
    // Half white over black, in the target's encoding (linear blend, stored sRGB).
    let middle = pixel(&data, 32, 32);
    assert!(middle[0] > 150 && middle[0] < 220, "{middle:?}");
    assert!(
        close(pixel(&data, 1, 1), [0, 0, 0, 255]),
        "{:?}",
        pixel(&data, 1, 1)
    );
}

#[test]
fn vector_icons_draw_in_their_color_and_share_one_batch() {
    let gpu = gpu();
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
    let mut text = text_store();
    let frame = noa_gpui_scene::VectorIcon::builder(10.0, 10.0)
        .fill_even_odd("M0 0H10V10H0Z M3 3H7V7H3Z")
        .unwrap()
        .build();
    let line = noa_gpui_scene::VectorIcon::builder(10.0, 10.0)
        .stroke("M1 5H9", 2.0)
        .unwrap()
        .build();

    let mut list = DisplayList::new();
    let red = Color::hex(0xff0000);
    list.push(noa_gpui_scene::DrawCommand::Icon {
        rect: Rect::new(0.0, 0.0, 20.0, 20.0),
        icon: frame.clone(),
        color: red,
    });
    list.push(noa_gpui_scene::DrawCommand::Icon {
        rect: Rect::new(30.0, 0.0, 20.0, 20.0),
        icon: line,
        color: Color::hex(0x0000ff),
    });
    list.push(noa_gpui_scene::DrawCommand::Icon {
        rect: Rect::new(0.0, 30.0, 30.0, 30.0),
        icon: frame,
        color: Color::hex(0x00ff00).with_alpha(0.5),
    });
    let data = render(&gpu, &mut renderer, &list, &mut text);

    assert!(close(pixel(&data, 2, 2), [255, 0, 0, 255]), "filled part");
    assert!(close(pixel(&data, 10, 10), WHITE), "the hole");
    assert!(close(pixel(&data, 40, 10), [0, 0, 255, 255]), "stroke");
    assert!(close(pixel(&data, 40, 4), WHITE), "beside the stroke");
    let half_green = pixel(&data, 3, 33);
    assert!(close(half_green, [188, 255, 188, 255]), "{half_green:?}");
    assert!(close(pixel(&data, 15, 45), WHITE), "the larger hole");

    let stats = renderer.stats();
    assert_eq!((stats.icons, stats.batches), (3, 1));
}

#[test]
fn a_full_icon_atlas_starts_over_with_the_icons_in_use() {
    let gpu = gpu();
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
    let mut text = text_store();
    // 40 icons of 120 × 120 pixels each fill most of a 1024 atlas; two such frames
    // with different icons do not fit together.
    let frame = |color: Color| {
        let mut list = DisplayList::new();
        for i in 0..40 {
            let icon = noa_gpui_scene::VectorIcon::builder(1.0, 1.0)
                .fill("M0 0H1V1H0Z")
                .unwrap()
                .build();
            list.push(noa_gpui_scene::DrawCommand::Icon {
                rect: Rect::new((i % 8) as f32, (i / 8) as f32, 120.0, 120.0),
                icon,
                color,
            });
        }
        list
    };
    render(&gpu, &mut renderer, &frame(Color::hex(0xff0000)), &mut text);
    assert_eq!(renderer.stats().icons, 40);
    let data = render(&gpu, &mut renderer, &frame(Color::hex(0x0000ff)), &mut text);
    assert_eq!(
        renderer.stats().icons,
        40,
        "every icon of the new frame drawn"
    );
    assert!(close(pixel(&data, 30, 30), [0, 0, 255, 255]));
}

#[test]
fn offscreen_frames_read_back_at_any_width_and_encode_as_png() {
    // OffscreenRenderer makes its own device. Locals drop in reverse order, so the turn
    // is released after the renderer.
    let _turn = turn();
    let mut offscreen = pollster::block_on(noa_gpui_render_wgpu::OffscreenRenderer::new())
        .expect("a GPU adapter is required for render tests");
    let mut text = text_store();
    // 70 pixels: rows of 280 bytes, padded to 512 for the copy.
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(0.0, 0.0, 35.0, 30.0), Color::hex(0xff0000));
    let viewport = Viewport {
        width: 70,
        height: 30,
        scale_factor: 1.0,
    };
    let image = offscreen
        .render(&list, &mut text, viewport, Color::WHITE)
        .expect("render");
    assert_eq!((image.width, image.height), (70, 30));
    assert_eq!(image.pixels.len(), 70 * 30 * 4);
    assert!(close(image.pixel(10, 29), [255, 0, 0, 255]), "quad");
    assert!(close(image.pixel(69, 0), WHITE), "background");
    let png = image.to_png();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&png[16..24], &[0, 0, 0, 70, 0, 0, 0, 30], "IHDR size");
}
