//! Built on macOS and Windows. Linux has its own tray icon (`linux_tray.rs`), and the
//! catalog draws its menu bar in the window there.

mod native {
    use noa_gpui_core::widgets::MenuItem as Item;
    use noa_gpui_text::TextStore;
    use tray_icon::menu::{
        CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
    };
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    use crate::app::AppOptions;
    use crate::screen::Screen;

    /// Native menu item ids: `app/<menu>/<index>` and `status/<index>`.
    #[cfg(target_os = "macos")]
    const APP: &str = "app/";
    const STATUS: &str = "status/";

    pub struct SystemMenus {
        name: String,
        #[cfg(target_os = "macos")]
        version: Option<String>,
        #[cfg(target_os = "macos")]
        app: Option<Menu>,
        tray: Option<TrayIcon>,
        built: bool,
    }

    impl SystemMenus {
        pub fn new(options: &AppOptions) -> Self {
            Self {
                name: options.name.clone(),
                #[cfg(target_os = "macos")]
                version: options.version.clone(),
                #[cfg(target_os = "macos")]
                app: None,
                tray: None,
                built: false,
            }
        }

        pub fn sync(&mut self, screen: &mut dyn Screen) {
            let changed = screen.take_system_menus_changed();
            if self.built && !changed {
                return;
            }
            self.built = true;
            #[cfg(target_os = "macos")]
            {
                let menus = screen.app_menus();
                if !menus.is_empty() {
                    let bar = app_menu_bar(&self.name, self.version.as_deref(), &menus);
                    bar.init_for_nsapp();
                    let titles: Vec<String> = bar
                        .items()
                        .iter()
                        .filter_map(|k| k.as_submenu().map(Submenu::text))
                        .collect();
                    log::debug!("system menu bar: {}", titles.join(" | "));
                    self.app = Some(bar);
                    screen.use_system_menu_bar();
                }
            }
            match screen.status_menu() {
                Some(items) => {
                    let menu = Menu::new();
                    let mut index = 0;
                    for item in build(&items, STATUS, &mut index, false) {
                        menu.append(item.as_ref()).ok();
                    }
                    match &self.tray {
                        Some(tray) => tray.set_menu(Some(Box::new(menu))),
                        None => {
                            self.tray = TrayIconBuilder::new()
                                .with_menu(Box::new(menu))
                                .with_icon(icon())
                                .with_icon_as_template(true)
                                .with_tooltip(&self.name)
                                .build()
                                .map_err(|e| log::warn!("menu bar extra: {e}"))
                                .ok();
                        }
                    }
                }
                None => self.tray = None,
            }
        }

        /// Hand chosen items to the screen. Returns whether there were any.
        pub fn poll(&mut self, screen: &mut dyn Screen, text: &mut TextStore) -> bool {
            let mut any = false;
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                let id = event.id().0.as_str();
                #[cfg(target_os = "macos")]
                if let Some(rest) = id.strip_prefix(APP) {
                    let mut parts = rest.split('/').filter_map(|p| p.parse().ok());
                    if let (Some(menu), Some(item)) = (parts.next(), parts.next()) {
                        screen.app_menu_chosen(menu, item, text);
                        any = true;
                    }
                    continue;
                }
                if let Some(item) = id.strip_prefix(STATUS).and_then(|i| i.parse().ok()) {
                    screen.status_menu_chosen(item, text);
                    any = true;
                }
            }
            if any {
                self.sync(screen);
            }
            any
        }
    }

    /// The menu bar: the application menu, the screen's menus, and a Window menu.
    #[cfg(target_os = "macos")]
    fn app_menu_bar(
        name: &str,
        version: Option<&str>,
        menus: &[noa_gpui_core::widgets::MenuBarMenu],
    ) -> Menu {
        use tray_icon::menu::AboutMetadata;
        let bar = Menu::new();
        let about = AboutMetadata {
            name: Some(name.into()),
            version: version.map(Into::into),
            ..Default::default()
        };
        let app = Submenu::with_items(
            name,
            true,
            &[
                &PredefinedMenuItem::about(None, Some(about)),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(None),
            ],
        )
        .expect("app menu");
        bar.append(&app).ok();
        let mut help = None;
        for (m, menu) in menus.iter().enumerate() {
            let submenu = Submenu::new(&menu.title, true);
            let mut index = 0;
            for item in build(&menu.items, &format!("{APP}{m}/"), &mut index, true) {
                submenu.append(item.as_ref()).ok();
            }
            bar.append(&submenu).ok();
            if menu.title == "Help" {
                help = Some(submenu);
            }
        }
        let window = Submenu::with_items(
            "Window",
            true,
            &[
                &PredefinedMenuItem::minimize(None),
                &PredefinedMenuItem::maximize(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::bring_all_to_front(None),
            ],
        )
        .expect("window menu");
        // The Window menu goes before Help, as in other Mac apps.
        match &help {
            Some(help) => {
                bar.remove(help).ok();
                bar.append(&window).ok();
                bar.append(help).ok();
                help.set_as_help_menu_for_nsapp();
            }
            None => {
                bar.append(&window).ok();
            }
        }
        window.set_as_windows_menu_for_nsapp();
        bar
    }

    /// Native items for `items`, numbered depth first from `*index` with ids
    /// `prefix<index>`.
    fn build(
        items: &[Item],
        prefix: &str,
        index: &mut usize,
        accelerators: bool,
    ) -> Vec<Box<dyn IsMenuItem>> {
        let mut out: Vec<Box<dyn IsMenuItem>> = Vec::new();
        for item in items {
            let id = format!("{prefix}{index}");
            *index += 1;
            if item.is_separator() {
                out.push(Box::new(PredefinedMenuItem::separator()));
                continue;
            }
            let accelerator = accelerators
                .then(|| item.shortcut_key().and_then(accelerator))
                .flatten();
            if !item.submenu_items().is_empty() {
                let submenu = Submenu::with_id(id, item.label(), item.is_enabled());
                for child in build(item.submenu_items(), prefix, index, accelerators) {
                    submenu.append(child.as_ref()).ok();
                }
                out.push(Box::new(submenu));
            } else if item.is_checked() {
                out.push(Box::new(CheckMenuItem::with_id(
                    id,
                    item.label(),
                    item.is_enabled(),
                    true,
                    accelerator,
                )));
            } else {
                out.push(Box::new(MenuItem::with_id(
                    id,
                    item.label(),
                    item.is_enabled(),
                    accelerator,
                )));
            }
        }
        out
    }

    /// The native accelerator for a shortcut; the command key is Cmd on macOS and Ctrl on
    /// Windows.
    fn accelerator(
        shortcut: noa_gpui_core::Shortcut,
    ) -> Option<tray_icon::menu::accelerator::Accelerator> {
        use noa_gpui_core::Key;
        use tray_icon::menu::accelerator::{Accelerator, Code, Modifiers};
        let code = match shortcut.key {
            Key::Char(c) => match c {
                'a'..='z' => {
                    let name = format!("Key{}", c.to_ascii_uppercase());
                    name.parse::<Code>().ok()?
                }
                '0'..='9' => format!("Digit{c}").parse::<Code>().ok()?,
                ',' => Code::Comma,
                '.' => Code::Period,
                '/' => Code::Slash,
                ';' => Code::Semicolon,
                '\'' => Code::Quote,
                '[' => Code::BracketLeft,
                ']' => Code::BracketRight,
                '-' => Code::Minus,
                '=' => Code::Equal,
                '`' => Code::Backquote,
                '\\' => Code::Backslash,
                _ => return None,
            },
            Key::Enter => Code::Enter,
            Key::Tab => Code::Tab,
            Key::Space => Code::Space,
            Key::Escape => Code::Escape,
            Key::Backspace => Code::Backspace,
            Key::Delete => Code::Delete,
            Key::ArrowLeft => Code::ArrowLeft,
            Key::ArrowRight => Code::ArrowRight,
            Key::ArrowUp => Code::ArrowUp,
            Key::ArrowDown => Code::ArrowDown,
            Key::Home => Code::Home,
            Key::End => Code::End,
            Key::PageUp => Code::PageUp,
            Key::PageDown => Code::PageDown,
            Key::F1 => Code::F1,
            Key::F2 => Code::F2,
            Key::F3 => Code::F3,
            Key::F4 => Code::F4,
            Key::F5 => Code::F5,
            Key::F6 => Code::F6,
            Key::F7 => Code::F7,
            Key::F8 => Code::F8,
            Key::F9 => Code::F9,
            Key::F10 => Code::F10,
            Key::F11 => Code::F11,
            Key::F12 => Code::F12,
            Key::ContextMenu | Key::Other => return None,
        };
        let mut mods = Modifiers::empty();
        if shortcut.command {
            mods |= if cfg!(target_os = "macos") {
                Modifiers::META
            } else {
                Modifiers::CONTROL
            };
        }
        if shortcut.shift {
            mods |= Modifiers::SHIFT;
        }
        if shortcut.alt {
            mods |= Modifiers::ALT;
        }
        Some(Accelerator::new(mods, code))
    }

    /// A ring with a dot: drawn as a template image, so macOS tints it for the menu bar.
    fn icon() -> Icon {
        const SIZE: u32 = 36;
        let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
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
                rgba.extend_from_slice(&[0, 0, 0, (a * 255.0) as u8]);
            }
        }
        Icon::from_rgba(rgba, SIZE, SIZE).expect("valid icon")
    }
}

pub use native::SystemMenus;
