use noa_gpui_text::{FontOptions, TextStore, TextStyle};

/// The Japanese font the Web demo loads (apps/web/fonts).
const JAPANESE_FONT: &[u8] = include_bytes!("../../../apps/web/fonts/NotoSansJP-Regular.otf");

/// Glyph ids of `text` as laid out; 0 is the missing-glyph box.
fn glyph_ids(store: &mut TextStore, text: &str) -> Vec<u16> {
    let id = store.create(text, TextStyle::new(16.0), None);
    let ids = store
        .layouts()
        .buffer(id)
        .layout_runs()
        .flat_map(|run| run.glyphs.iter().map(|g| g.glyph_id))
        .collect();
    store.remove(id);
    ids
}

#[test]
fn a_loaded_font_covers_what_the_bundled_one_lacks() {
    // As on the Web: no system fonts, only the bundled Latin font.
    let mut store = TextStore::new(FontOptions {
        locale: "ja-JP".into(),
        load_system_fonts: false,
    });
    assert!(
        glyph_ids(&mut store, "日本語").iter().all(|&g| g == 0),
        "the bundled font has no Japanese glyphs"
    );

    store.load_font(JAPANESE_FONT.to_vec());
    assert!(store.families().iter().any(|f| f == "Noto Sans JP"));
    let glyphs = glyph_ids(&mut store, "日本語");
    assert_eq!(glyphs.len(), 3);
    assert!(glyphs.iter().all(|&g| g != 0), "{glyphs:?}");

    // Only a regular weight is loaded: bold Japanese text still finds its glyphs there.
    let id = store.create("日本語", TextStyle::new(16.0).weight(700), None);
    let bold: Vec<u16> = store
        .layouts()
        .buffer(id)
        .layout_runs()
        .flat_map(|run| run.glyphs.iter().map(|g| g.glyph_id))
        .collect();
    assert_eq!(bold, glyphs);
}

#[test]
fn system_ui_names_stand_for_one_family() {
    let mut store = TextStore::new(FontOptions {
        locale: "en-US".into(),
        load_system_fonts: false,
    });
    let system = store.font_family("system-ui");
    // Without system fonts (as on the Web) it is the bundled font.
    assert_eq!(store.family_name(system), "Inter");
    for name in [".SystemUIFont", "-apple-system", "BlinkMacSystemFont"] {
        assert_eq!(store.font_family(name), system, "{name}");
    }
}

/// San Francisco is one variable font file: the weight comes from its axis, so bold
/// text is wider than regular text at the same size.
#[cfg(target_os = "macos")]
#[test]
fn the_mac_system_font_is_san_francisco_with_variable_weight() {
    let mut store = TextStore::new(FontOptions {
        locale: "en-US".into(),
        load_system_fonts: true,
    });
    let system = store.font_family(".SystemUIFont");
    assert_eq!(store.family_name(system), "System Font");
    let mut width = |weight| {
        let style = TextStyle::new(13.0).family(system).weight(weight);
        let id = store.create("The quick brown fox jumps", style, None);
        let size = store.size(id);
        store.remove(id);
        size
    };
    let (regular, bold) = (width(400), width(700));
    assert!(bold.width > regular.width, "{bold:?} vs {regular:?}");
    // 13 pt with a line height of 1.3: one line of ceil(16.9) = 17 px.
    assert_eq!(regular.height, 17.0);
}
