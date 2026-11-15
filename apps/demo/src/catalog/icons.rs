//! Icons written as SVG the way icon sets ship them: stroked outlines (24-unit grid,
//! 2 wide, round caps) and filled glyphs.

use noa_gpui_scene::VectorIcon;

const HOUSE: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 10.5 12 3l9 7.5"/><path d="M5 9.5V20h14V9.5"/><path d="M10 20v-6h4v6"/></svg>"#;
const BELL: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9"/><path d="M10.3 21a1.94 1.94 0 0 0 3.4 0"/></svg>"#;
const SEARCH: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="11" cy="11" r="7"/><line x1="16.5" y1="16.5" x2="21" y2="21"/></svg>"#;
const GEAR: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1 7 17M17 7l2.1-2.1"/></svg>"#;
const HEART: &str = r#"<svg viewBox="0 0 24 24"><path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/></svg>"#;
const STAR: &str = r#"<svg viewBox="0 0 24 24"><path d="M12 17.27 18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z"/></svg>"#;
const FOLDER: &str = r#"<svg viewBox="0 0 24 24"><path d="M10 4H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2h-8l-2-2z"/></svg>"#;
const RING: &str = r#"<svg viewBox="0 0 24 24"><path fill-rule="evenodd" d="M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20zm0 5a5 5 0 1 0 0 10 5 5 0 1 0 0-10z"/></svg>"#;

pub struct Icons {
    pub house: VectorIcon,
    pub bell: VectorIcon,
    pub search: VectorIcon,
    pub gear: VectorIcon,
    pub heart: VectorIcon,
    pub star: VectorIcon,
    pub folder: VectorIcon,
    pub ring: VectorIcon,
}

impl Icons {
    pub fn load() -> Self {
        let svg = |s: &str| VectorIcon::from_svg(s).expect("the catalog's icons are valid SVG");
        Self {
            house: svg(HOUSE),
            bell: svg(BELL),
            search: svg(SEARCH),
            gear: svg(GEAR),
            heart: svg(HEART),
            star: svg(STAR),
            folder: svg(FOLDER),
            ring: svg(RING),
        }
    }
}
