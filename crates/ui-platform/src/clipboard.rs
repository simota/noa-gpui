use noa_gpui_core::clipboard::Clipboard;

/// Reads and writes the system clipboard. If the clipboard cannot be opened (for
/// example, no display server), copied text stays inside the app.
pub struct SystemClipboard {
    system: Option<arboard::Clipboard>,
    fallback: noa_gpui_core::clipboard::MemoryClipboard,
}

impl SystemClipboard {
    pub fn new() -> Self {
        let system = arboard::Clipboard::new()
            .inspect_err(|e| log::warn!("system clipboard unavailable, using an in-app one: {e}"))
            .ok();
        Self {
            system,
            fallback: Default::default(),
        }
    }
}

impl Clipboard for SystemClipboard {
    fn read_text(&mut self) -> Option<String> {
        match &mut self.system {
            Some(system) => system.get_text().ok(),
            None => self.fallback.read_text(),
        }
    }

    fn write_text(&mut self, text: &str) {
        match &mut self.system {
            Some(system) => {
                if let Err(e) = system.set_text(text) {
                    log::warn!("could not write the clipboard: {e}");
                }
            }
            None => self.fallback.write_text(text),
        }
    }
}
