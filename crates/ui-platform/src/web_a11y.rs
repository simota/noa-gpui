//! Accessibility on the Web: the AccessKit tree mirrored as ARIA elements inside the
//! canvas, as its fallback content. Browsers expose a rendered canvas's fallback
//! content to assistive technology, and events on it bubble to the canvas, so keys
//! typed while a mirrored element has focus still reach winit's listeners.
//!
//! [`AriaMirror`] decides what changes; this module only carries the operations out.
//! The widget with keyboard focus is announced through the canvas's
//! `aria-activedescendant`, so DOM focus stays on the canvas (or the IME text area).
//! A click on a mirrored element, or assistive technology moving focus to one, is
//! queued as an AccessKit action and carried out on the next redraw, as on the desktop.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use noa_gpui_core::UiTree;
use noa_gpui_core::accesskit::{Action, ActionRequest, NodeId, TreeId};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{Document, Element, FocusOptions, HtmlCanvasElement};

use crate::wake::Waker;
use crate::web_aria::{AriaMirror, DomOp, ELEMENT_ID_PREFIX, element_id};

type Listener = Closure<dyn FnMut(web_sys::Event)>;

pub(crate) struct WebA11y {
    canvas: HtmlCanvasElement,
    document: Document,
    mirror: AriaMirror,
    elements: HashMap<u64, Element>,
    /// Each element's own text, kept as its first child so child elements stay.
    texts: HashMap<u64, web_sys::Text>,
    actions: Rc<RefCell<Vec<ActionRequest>>>,
    /// Whether the first, full tree was mirrored.
    built: bool,
    /// Removed from the canvas when this is dropped.
    listeners: Vec<(&'static str, Listener)>,
}

impl WebA11y {
    /// `None` when the page has no document.
    pub fn new(canvas: &HtmlCanvasElement, waker: &Waker) -> Option<Self> {
        let document = web_sys::window()?.document()?;
        // A role that takes `aria-activedescendant`; the page may have chosen its own.
        if !canvas.has_attribute("role") {
            let _ = canvas.set_attribute("role", "application");
        }
        let actions = Rc::new(RefCell::new(Vec::new()));
        let listen = |action: Action, refocus: bool| {
            let (actions, waker, canvas) = (actions.clone(), waker.clone(), canvas.clone());
            Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
                let Some(node) = mirrored_node(&event) else {
                    return;
                };
                actions.borrow_mut().push(ActionRequest {
                    action,
                    target_tree: TreeId::ROOT,
                    target_node: node,
                    data: None,
                });
                if refocus {
                    // The canvas keeps DOM focus; the focused widget is announced
                    // through `aria-activedescendant`.
                    let options = FocusOptions::new();
                    options.set_prevent_scroll(true);
                    let _ = canvas.focus_with_options(&options);
                }
                waker.nudge();
            })
        };
        let listeners = vec![
            ("click", listen(Action::Click, false)),
            ("focusin", listen(Action::Focus, true)),
        ];
        for (name, listener) in &listeners {
            let _ =
                canvas.add_event_listener_with_callback(name, listener.as_ref().unchecked_ref());
        }
        Some(Self {
            canvas: canvas.clone(),
            document,
            mirror: AriaMirror::default(),
            elements: HashMap::new(),
            texts: HashMap::new(),
            actions,
            built: false,
            listeners,
        })
    }

    pub fn has_actions(&self) -> bool {
        !self.actions.borrow().is_empty()
    }

    pub fn take_actions(&self) -> Vec<ActionRequest> {
        std::mem::take(&mut *self.actions.borrow_mut())
    }

    /// Bring the elements up to date when the tree `changed` (or was never mirrored).
    pub fn update(&mut self, tree: &UiTree, changed: bool) {
        if !changed && self.built {
            return;
        }
        self.built = true;
        // Bounds are not used: fallback content is not laid out.
        let update = tree.accessibility_update(1.0);
        for op in self.mirror.update(&update) {
            self.apply(op);
        }
    }

    fn apply(&mut self, op: DomOp) {
        match op {
            DomOp::Create(node) => {
                if let Ok(element) = self.document.create_element("div") {
                    self.elements.insert(node, element);
                }
            }
            DomOp::Attr { node, name, value } => {
                let Some(element) = self.elements.get(&node) else {
                    return;
                };
                let _ = match value {
                    Some(value) => element.set_attribute(name, &value),
                    None => element.remove_attribute(name),
                };
            }
            DomOp::Text { node, text } => {
                let Some(element) = self.elements.get(&node) else {
                    return;
                };
                if let Some(existing) = self.texts.get(&node) {
                    existing.set_data(&text);
                } else {
                    let created = self.document.create_text_node(&text);
                    let _ = element.prepend_with_node_1(&created);
                    self.texts.insert(node, created);
                }
            }
            DomOp::Append { parent, node } => {
                let Some(element) = self.elements.get(&node) else {
                    return;
                };
                let parent: &web_sys::Node = match parent {
                    None => &self.canvas,
                    Some(parent) => match self.elements.get(&parent) {
                        Some(parent) => parent,
                        None => return,
                    },
                };
                let _ = parent.append_child(element);
            }
            DomOp::Remove(node) => {
                if let Some(element) = self.elements.remove(&node) {
                    element.remove();
                }
                self.texts.remove(&node);
            }
            DomOp::Focus(node) => {
                let _ = match node {
                    Some(node) => self
                        .canvas
                        .set_attribute("aria-activedescendant", &element_id(node)),
                    None => self.canvas.remove_attribute("aria-activedescendant"),
                };
            }
        }
    }
}

/// The node of the mirrored element the event happened on (or inside).
fn mirrored_node(event: &web_sys::Event) -> Option<NodeId> {
    let target: Element = event.target()?.dyn_into().ok()?;
    let element = target
        .closest(&format!("[id^=\"{ELEMENT_ID_PREFIX}\"]"))
        .ok()??;
    let id = element.id();
    let node = id.strip_prefix(ELEMENT_ID_PREFIX)?.parse().ok()?;
    Some(NodeId(node))
}

impl Drop for WebA11y {
    fn drop(&mut self) {
        for (name, listener) in &self.listeners {
            let _ = self
                .canvas
                .remove_event_listener_with_callback(name, listener.as_ref().unchecked_ref());
        }
        for element in self.elements.values() {
            element.remove();
        }
    }
}
