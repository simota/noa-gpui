use noa_gpui_text::{FontOptions, TextStore, TextStyle};

#[test]
fn independent_stores_never_reuse_layout_revisions() {
    let make = || {
        TextStore::new(FontOptions {
            locale: "en-US".into(),
            load_system_fonts: false,
        })
    };
    let (mut first, mut second) = (make(), make());
    let a = first.create("first", TextStyle::new(16.0), None);
    let b = second.create("second", TextStyle::new(16.0), None);
    assert_eq!(a, b, "layout IDs are local to each store");
    assert_ne!(first.layouts().revision(), second.layouts().revision());

    let before = first.layouts().revision();
    assert_eq!(first.layouts().revision(), before, "reads are stable");
    assert!(first.rewrap(a, TextStyle::new(16.0), Some(10.0)));
    let wrapped = first.layouts().revision();
    assert_ne!(wrapped, before);
    assert_ne!(wrapped, second.layouts().revision());

    first.remove(a);
    let removed = first.layouts().revision();
    assert_ne!(removed, wrapped);
    assert_eq!(first.create("third", TextStyle::new(16.0), None), a);
    assert_ne!(first.layouts().revision(), removed);
    assert_ne!(first.layouts().revision(), second.layouts().revision());
}

#[test]
fn changes_count_this_stores_creations_rewraps_and_removals() {
    let make = || {
        TextStore::new(FontOptions {
            locale: "en-US".into(),
            load_system_fonts: false,
        })
    };
    let (mut first, mut second) = (make(), make());
    let a = first.create("first", TextStyle::new(16.0), None);
    let _ = first.layouts().revision();
    second.create("second", TextStyle::new(16.0), None);
    let _ = second.layouts().revision();
    assert!(first.rewrap(a, TextStyle::new(16.0), Some(10.0)));
    first.remove(a);
    assert_eq!(first.layouts().changes(), 3);
    assert_eq!(second.layouts().changes(), 1);
}
