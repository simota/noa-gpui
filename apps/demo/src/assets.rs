//! Procedurally generated images, so the demo ships no image files.

use noa_gpui_render_wgpu::{Renderer, wgpu};
use noa_gpui_scene::ImageId;

const ICON: u32 = 32;

pub struct Assets {
    pub icons: [ImageId; 4],
    pub banner: ImageId,
}

impl Assets {
    pub fn load(renderer: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self::generate(|w, h, rgba| renderer.create_image(device, queue, w, h, rgba))
    }

    /// `create` receives width, height and RGBA8 pixels.
    pub fn generate(mut create: impl FnMut(u32, u32, &[u8]) -> ImageId) -> Self {
        let mut upload = |w: u32, h: u32, f: &dyn Fn(f32, f32) -> [u8; 4]| {
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h {
                for x in 0..w {
                    let u = (x as f32 + 0.5) / w as f32;
                    let v = (y as f32 + 0.5) / h as f32;
                    rgba.extend_from_slice(&f(u, v));
                }
            }
            create(w, h, &rgba)
        };

        let disc = upload(ICON, ICON, &|u, v| {
            let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt();
            let a = ((0.42 - d) * ICON as f32).clamp(0.0, 1.0);
            [255, 196, 0, (a * 255.0) as u8]
        });
        let ring = upload(ICON, ICON, &|u, v| {
            let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt();
            let a = ((0.08 - (d - 0.32).abs()) * ICON as f32).clamp(0.0, 1.0);
            [40, 200, 255, (a * 255.0) as u8]
        });
        let checker = upload(ICON, ICON, &|u, v| {
            let on = ((u * 4.0) as u32 + (v * 4.0) as u32).is_multiple_of(2);
            if on {
                [240, 80, 120, 255]
            } else {
                [255, 255, 255, 255]
            }
        });
        let gradient = upload(ICON, ICON, &|u, v| {
            [(u * 255.0) as u8, (v * 255.0) as u8, 200, 255]
        });
        let banner = upload(256, 64, &|u, v| {
            let wave = ((u * 18.0).sin() * 0.5 + 0.5) * 0.3;
            [
                (60.0 + 160.0 * u) as u8,
                (90.0 + 120.0 * (v + wave).min(1.0)) as u8,
                (220.0 - 120.0 * u) as u8,
                255,
            ]
        });

        Self {
            icons: [disc, ring, checker, gradient],
            banner,
        }
    }
}
