//! The tray icon as a StatusNotifierItem through `ksni`, without GTK. Desktops without a
//! StatusNotifierItem host show no icon.
//!
//! The app menu bar stays in the window on Linux (the catalog draws it).

use std::sync::mpsc::{Receiver, Sender, channel};

use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, MenuItem as Native, StandardItem, SubMenu};
use ui_core::widgets::MenuItem as Item;
use ui_text::TextStore;

use crate::app::AppOptions;
use crate::screen::Screen;
use crate::wake::Waker;

/// The tray icon's state, owned by ksni's D-Bus thread.
struct Tray {
    name: String,
    items: Vec<Item>,
    chosen: Sender<usize>,
    waker: Waker,
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        self.name.to_lowercase().replace(char::is_whitespace, "-")
    }

    fn title(&self) -> String {
        self.name.clone()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![icon()]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self.name.clone(),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<Native<Self>> {
        build(&self.items, &mut 0)
    }
}

/// Native items for `items`, numbered depth first from `*index` as the screen counts
/// them for [`Screen::status_menu_chosen`].
fn build(items: &[Item], index: &mut usize) -> Vec<Native<Tray>> {
    let mut out = Vec::new();
    for item in items {
        let i = *index;
        *index += 1;
        if item.is_separator() {
            out.push(Native::Separator);
            continue;
        }
        let activate = move |tray: &mut Tray| {
            let _ = tray.chosen.send(i);
            tray.waker.nudge();
        };
        if !item.submenu_items().is_empty() {
            out.push(Native::SubMenu(SubMenu {
                label: item.label().to_owned(),
                enabled: item.is_enabled(),
                submenu: build(item.submenu_items(), index),
                ..Default::default()
            }));
        } else if item.is_checked() {
            out.push(Native::Checkmark(CheckmarkItem {
                label: item.label().to_owned(),
                enabled: item.is_enabled(),
                checked: true,
                activate: Box::new(activate),
                ..Default::default()
            }));
        } else {
            out.push(Native::Standard(StandardItem {
                label: item.label().to_owned(),
                enabled: item.is_enabled(),
                activate: Box::new(activate),
                ..Default::default()
            }));
        }
    }
    out
}

/// A ring around a dot, in the panel's text gray (StatusNotifierItem has no template
/// icons that follow the panel's colors).
fn icon() -> ksni::Icon {
    const SIZE: i32 = 32;
    let mut argb = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (u, v) = (
                (x as f32 + 0.5) / SIZE as f32 - 0.5,
                (y as f32 + 0.5) / SIZE as f32 - 0.5,
            );
            let d = (u * u + v * v).sqrt();
            let ring = (0.06 - (d - 0.36).abs()) * SIZE as f32;
            let dot = (0.14 - d) * SIZE as f32;
            let a = ring.max(dot).clamp(0.0, 1.0);
            argb.extend_from_slice(&[(a * 255.0) as u8, 0xd0, 0xd0, 0xd0]);
        }
    }
    ksni::Icon {
        width: SIZE,
        height: SIZE,
        data: argb,
    }
}

pub struct SystemMenus {
    name: String,
    waker: Waker,
    tray: Option<Handle<Tray>>,
    chosen_tx: Sender<usize>,
    chosen: Receiver<usize>,
    built: bool,
    /// Starting the service failed (no D-Bus session): do not try again.
    unavailable: bool,
}

impl SystemMenus {
    pub fn new(options: &AppOptions, waker: &Waker) -> Self {
        let (chosen_tx, chosen) = channel();
        Self {
            name: options.name.clone(),
            waker: waker.clone(),
            tray: None,
            chosen_tx,
            chosen,
            built: false,
            unavailable: false,
        }
    }

    pub fn sync(&mut self, screen: &mut dyn Screen) {
        let changed = screen.take_system_menus_changed();
        if self.built && !changed {
            return;
        }
        self.built = true;
        match screen.status_menu() {
            Some(items) => match &self.tray {
                Some(tray) => {
                    tray.update(|t| t.items = items);
                }
                None if !self.unavailable => {
                    let tray = Tray {
                        name: self.name.clone(),
                        items,
                        chosen: self.chosen_tx.clone(),
                        waker: self.waker.clone(),
                    };
                    match tray.spawn() {
                        Ok(handle) => self.tray = Some(handle),
                        Err(e) => {
                            log::warn!("tray icon: {e}");
                            self.unavailable = true;
                        }
                    }
                }
                None => {}
            },
            None => {
                if let Some(tray) = self.tray.take() {
                    tray.shutdown().wait();
                }
            }
        }
    }

    /// Hand chosen items to the screen. Returns whether there were any.
    pub fn poll(&mut self, screen: &mut dyn Screen, text: &mut TextStore) -> bool {
        let mut any = false;
        while let Ok(item) = self.chosen.try_recv() {
            screen.status_menu_chosen(item, text);
            any = true;
        }
        if any {
            self.sync(screen);
        }
        any
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_are_numbered_depth_first_as_the_screen_counts_them() {
        let items = vec![
            Item::new("Open"),
            Item::separator(),
            Item::new("More").submenu(vec![Item::new("A"), Item::new("B").checked(true)]),
            Item::new("Quit").enabled(false),
        ];
        let (chosen, received) = channel();
        let mut tray = Tray {
            name: "Demo App".into(),
            items: items.clone(),
            chosen,
            waker: Waker::default(),
        };
        assert_eq!(ksni::Tray::id(&tray), "demo-app");
        let menu = build(&items, &mut 0);
        let Native::SubMenu(more) = &menu[2] else {
            panic!("submenu");
        };
        let Native::Checkmark(b) = &more.submenu[1] else {
            panic!("checked item");
        };
        assert!(b.checked);
        (b.activate)(&mut tray);
        let Native::Standard(quit) = &menu[3] else {
            panic!("item");
        };
        assert!(!quit.enabled);
        (quit.activate)(&mut tray);
        // "B" is item 4 (Open, separator, More, A, B), "Quit" item 5.
        assert_eq!(received.try_iter().collect::<Vec<_>>(), [4, 5]);
    }
}
