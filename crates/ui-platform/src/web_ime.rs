//! The hidden text area that takes keyboard and IME input on the Web while a text
//! widget has focus (winit's Web backend reports no IME events). It sits at the caret,
//! so the IME's candidate window opens there; its events become [`Event`]s through
//! [`WebTextInput`] and are queued for the event loop, which a nudge of the
//! [`Waker`] wakes.

use std::cell::RefCell;
use std::rc::Rc;

use noa_gpui_core::Event;
use noa_gpui_scene::Rect;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{
    CompositionEvent, FocusOptions, HtmlCanvasElement, HtmlTextAreaElement, InputEvent,
    KeyboardEvent,
};

use crate::wake::Waker;
use crate::web_text::{DomKey, DomModifiers, WebTextInput};

type Listener = Closure<dyn FnMut(web_sys::Event)>;
/// Turns one DOM event of the text area into input events.
type Handler = Box<dyn Fn(&web_sys::Event, &mut WebTextInput) -> Vec<Event>>;

pub(crate) struct WebIme {
    textarea: HtmlTextAreaElement,
    canvas: HtmlCanvasElement,
    queue: Rc<RefCell<Vec<Event>>>,
    /// The caret area the text area was last placed at; `None` while no text widget
    /// has focus.
    area: Option<Rect>,
    /// Removed from the text area when this is dropped.
    listeners: Vec<(&'static str, Listener)>,
}

impl WebIme {
    /// `None` when the page has no document to put the text area in.
    pub fn new(canvas: &HtmlCanvasElement, waker: &Waker) -> Option<Self> {
        let document = web_sys::window()?.document()?;
        let textarea: HtmlTextAreaElement =
            document.create_element("textarea").ok()?.dyn_into().ok()?;
        for (name, value) in [
            ("autocomplete", "off"),
            ("autocorrect", "off"),
            ("autocapitalize", "off"),
            ("spellcheck", "false"),
            ("tabindex", "-1"),
        ] {
            let _ = textarea.set_attribute(name, value);
        }
        let style = textarea.style();
        for (name, value) in [
            ("position", "fixed"),
            ("left", "0"),
            ("top", "0"),
            ("width", "1px"),
            ("height", "1px"),
            ("padding", "0"),
            ("border", "0"),
            ("margin", "0"),
            ("outline", "none"),
            ("resize", "none"),
            ("overflow", "hidden"),
            ("opacity", "0"),
            ("pointer-events", "none"),
            ("caret-color", "transparent"),
        ] {
            let _ = style.set_property(name, value);
        }
        document.body()?.append_child(&textarea).ok()?;

        let state = Rc::new(RefCell::new(WebTextInput::new(crate::command_is_meta())));
        let queue = Rc::new(RefCell::new(Vec::new()));
        let listen = |handle: Handler| {
            let (state, queue, waker) = (state.clone(), queue.clone(), waker.clone());
            Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
                let events = handle(&event, &mut state.borrow_mut());
                if !events.is_empty() {
                    queue.borrow_mut().extend(events);
                    waker.nudge();
                }
            })
        };
        let field = textarea.clone();
        // The text area only passes text through: it is emptied once the text is taken.
        let clear = move || field.set_value("");
        let clear_after_input = clear.clone();
        let listeners: Vec<(&'static str, Listener)> = vec![
            (
                "keydown",
                listen(Box::new(|event, input| {
                    let Some(event) = event.dyn_ref::<KeyboardEvent>() else {
                        return vec![];
                    };
                    let outcome = input.key_down(dom_key(event, &event.key()));
                    if outcome.prevent_default {
                        event.prevent_default();
                    }
                    outcome.events
                })),
            ),
            (
                "keyup",
                listen(Box::new(|event, input| {
                    let Some(event) = event.dyn_ref::<KeyboardEvent>() else {
                        return vec![];
                    };
                    input.key_up(dom_key(event, &event.key()))
                })),
            ),
            (
                "input",
                listen(Box::new(move |event, input| {
                    let Some(event) = event.dyn_ref::<InputEvent>() else {
                        return vec![];
                    };
                    if !event.is_composing() {
                        clear_after_input();
                    }
                    input.input(
                        &event.input_type(),
                        event.data().as_deref(),
                        event.is_composing(),
                    )
                })),
            ),
            (
                "compositionupdate",
                listen(Box::new(|event, input| {
                    let data = event
                        .dyn_ref::<CompositionEvent>()
                        .and_then(CompositionEvent::data)
                        .unwrap_or_default();
                    input.composition_update(&data)
                })),
            ),
            (
                "compositionend",
                listen(Box::new(move |event, input| {
                    let data = event
                        .dyn_ref::<CompositionEvent>()
                        .and_then(CompositionEvent::data)
                        .unwrap_or_default();
                    clear();
                    input.composition_end(&data)
                })),
            ),
        ];
        for (name, listener) in &listeners {
            let _ =
                textarea.add_event_listener_with_callback(name, listener.as_ref().unchecked_ref());
        }
        Some(Self {
            textarea,
            canvas: canvas.clone(),
            queue,
            area: None,
            listeners,
        })
    }

    /// The events the text area produced since the last call.
    pub fn take_events(&self) -> Vec<Event> {
        std::mem::take(&mut self.queue.borrow_mut())
    }

    /// Keep the text area focused at the caret while a text widget has focus (`area`,
    /// in logical pixels of the canvas), and give focus back to the canvas when none
    /// has. A click on the canvas focuses the canvas, so this runs after input too.
    /// Name the text area after the text widget it stands in for (`None`: no name), so
    /// assistive technology announces that widget while the text area has focus.
    pub fn set_label(&self, label: Option<&str>) {
        let current = self.textarea.get_attribute("aria-label");
        if current.as_deref() == label {
            return;
        }
        let _ = match label {
            Some(label) => self.textarea.set_attribute("aria-label", label),
            None => self.textarea.remove_attribute("aria-label"),
        };
    }

    pub fn sync(&mut self, area: Option<Rect>) {
        match area {
            Some(rect) => {
                if self.area != Some(rect) {
                    let canvas = self.canvas.get_bounding_client_rect();
                    let style = self.textarea.style();
                    let px = |v: f64| format!("{v}px");
                    let _ = style.set_property("left", &px(canvas.left() + f64::from(rect.x)));
                    let _ = style.set_property("top", &px(canvas.top() + f64::from(rect.y)));
                    let _ = style.set_property("height", &px(f64::from(rect.height)));
                    let _ = style.set_property("font-size", &px(f64::from(rect.height)));
                }
                if !self.focused() {
                    let options = FocusOptions::new();
                    options.set_prevent_scroll(true);
                    let _ = self.textarea.focus_with_options(&options);
                }
            }
            None if self.area.is_some() => {
                if self.focused() {
                    let _ = self.canvas.focus();
                }
                self.textarea.set_value("");
            }
            None => {}
        }
        self.area = area;
    }

    fn focused(&self) -> bool {
        self.textarea
            .owner_document()
            .and_then(|d| d.active_element())
            .is_some_and(|e| e == **self.textarea)
    }
}

impl Drop for WebIme {
    fn drop(&mut self) {
        for (name, listener) in &self.listeners {
            let _ = self
                .textarea
                .remove_event_listener_with_callback(name, listener.as_ref().unchecked_ref());
        }
        self.textarea.remove();
    }
}

fn dom_key<'a>(event: &KeyboardEvent, key: &'a str) -> DomKey<'a> {
    DomKey {
        key,
        key_code: event.key_code(),
        composing: event.is_composing(),
        modifiers: DomModifiers {
            shift: event.shift_key(),
            ctrl: event.ctrl_key(),
            alt: event.alt_key(),
            meta: event.meta_key(),
        },
    }
}
