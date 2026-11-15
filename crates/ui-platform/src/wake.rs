use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use winit::event_loop::EventLoopProxy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wake;

/// Wakes the event loop from any thread. The loop then calls [`crate::Screen::woken`]
/// on the main screen.
///
/// Wakes that come before the screen has handled the last one are merged into one
/// call. A wake before the event loop runs is delivered once it does; a wake after
/// the loop has ended does nothing.
#[derive(Clone, Default)]
pub struct Waker(Arc<Shared>);

#[derive(Default)]
struct Shared {
    pending: AtomicBool,
    proxy: Mutex<Option<EventLoopProxy<Wake>>>,
}

impl std::fmt::Debug for Waker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Waker")
            .field("pending", &self.0.pending.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl Waker {
    pub fn wake(&self) {
        if self.0.pending.swap(true, Ordering::AcqRel) {
            // The loop has not taken the last wake yet; it will see this one too.
            return;
        }
        self.send();
    }

    /// Wake the event loop without a [`crate::Screen::woken`] call, for the app's own
    /// events (a reopen, a tray menu item, typing on the Web).
    #[cfg_attr(
        not(any(
            all(feature = "native", any(target_os = "macos", target_os = "linux")),
            target_arch = "wasm32"
        )),
        expect(dead_code)
    )]
    pub(crate) fn nudge(&self) {
        self.send();
    }

    fn send(&self) {
        let proxy = self.0.proxy.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(proxy) = proxy.as_ref() {
            // Fails only once the loop has ended, when there is nothing left to wake.
            let _ = proxy.send_event(Wake);
        }
    }

    /// Send wakes to the event loop of `proxy`, including one that came before.
    pub(crate) fn connect(&self, proxy: EventLoopProxy<Wake>) {
        *self.0.proxy.lock().unwrap_or_else(|e| e.into_inner()) = Some(proxy);
        if self.0.pending.load(Ordering::Acquire) {
            self.send();
        }
    }

    /// Whether the waker was woken since the last call.
    pub(crate) fn take(&self) -> bool {
        self.0.pending.swap(false, Ordering::AcqRel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wakes_from_other_threads_are_merged_until_taken() {
        let waker = Waker::default();
        assert!(!waker.take());
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let waker = waker.clone();
                std::thread::spawn(move || waker.wake())
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert!(waker.take());
        assert!(!waker.take(), "four wakes, one call");
        waker.wake();
        assert!(waker.take());
    }

    #[test]
    fn a_waker_can_be_sent_to_and_shared_between_threads() {
        fn send_sync<T: Send + Sync + 'static>() {}
        send_sync::<Waker>();
    }
}
