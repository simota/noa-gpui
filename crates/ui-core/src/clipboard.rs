//! The application installs the OS clipboard with [`crate::UiTree::set_clipboard`].
//! Until then a [`MemoryClipboard`] keeps copied text inside the application.

pub trait Clipboard {
    fn read_text(&mut self) -> Option<String>;
    fn write_text(&mut self, text: &str);
}

#[derive(Debug, Default, Clone)]
pub struct MemoryClipboard {
    text: Option<String>,
}

impl Clipboard for MemoryClipboard {
    fn read_text(&mut self) -> Option<String> {
        self.text.clone()
    }

    fn write_text(&mut self, text: &str) {
        self.text = Some(text.to_owned());
    }
}
