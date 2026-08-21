//! Browser display-sleep guard: the Screen Wake Lock API
//! (`navigator.wakeLock.request("screen")`) held while an exercise runs —
//! the port of `DisplaySleepGuard.swift`'s IOKit power assertion.
//!
//! The browser drops the lock whenever the tab is hidden, so a
//! `visibilitychange` listener re-acquires it while the guard is wanted.
//! web-sys gates `WakeLock` behind `web_sys_unstable_apis`; the calls go
//! through `js_sys::Reflect` instead so the build stays on stable
//! bindings (browsers without the API just log once and carry on).

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::{Function, Promise, Reflect};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};

#[derive(Default)]
struct WakeState {
    /// Whether the app wants the display awake right now.
    wanted: bool,
    /// The held `WakeLockSentinel`, when acquired.
    sentinel: Option<JsValue>,
    /// A request is in flight (don't stack them).
    requesting: bool,
    on_visibility: Option<Closure<dyn FnMut(web_sys::Event)>>,
    warned: bool,
}

pub struct ScreenWakeLock {
    state: Rc<RefCell<WakeState>>,
}

impl ScreenWakeLock {
    pub fn new() -> Self {
        let state = Rc::new(RefCell::new(WakeState::default()));
        // Re-acquire when the tab comes back (the browser releases the
        // lock on hide).
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            let state_for_visibility = Rc::clone(&state);
            let on_visibility = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                let visible = web_sys::window()
                    .and_then(|w| w.document())
                    .is_some_and(|d| d.visibility_state() == web_sys::VisibilityState::Visible);
                let wanted = state_for_visibility.borrow().wanted;
                if visible && wanted {
                    Self::acquire(&state_for_visibility);
                }
            });
            let _ = document.add_event_listener_with_callback(
                "visibilitychange",
                on_visibility.as_ref().unchecked_ref(),
            );
            state.borrow_mut().on_visibility = Some(on_visibility);
        }
        Self { state }
    }

    pub fn set_active(&self, active: bool) {
        {
            let mut state = self.state.borrow_mut();
            if state.wanted == active {
                return;
            }
            state.wanted = active;
        }
        if active {
            Self::acquire(&self.state);
        } else {
            Self::release(&self.state);
        }
    }

    /// `navigator.wakeLock`, if the browser has it.
    fn wake_lock_object() -> Option<JsValue> {
        let navigator = web_sys::window()?.navigator();
        let wake_lock = Reflect::get(&navigator, &"wakeLock".into()).ok()?;
        (!wake_lock.is_undefined() && !wake_lock.is_null()).then_some(wake_lock)
    }

    fn acquire(state: &Rc<RefCell<WakeState>>) {
        {
            let state_ref = state.borrow();
            if state_ref.requesting {
                return;
            }
            if let Some(sentinel) = &state_ref.sentinel {
                // Still held (the browser marks dropped locks `released`).
                let released = Reflect::get(sentinel, &"released".into())
                    .ok()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                if !released {
                    return;
                }
            }
        }
        let Some(wake_lock) = Self::wake_lock_object() else {
            let mut state = state.borrow_mut();
            if !state.warned {
                state.warned = true;
                web_sys::console::warn_1(
                    &"KeyInSight: Screen Wake Lock unavailable — the display may sleep mid-exercise"
                        .into(),
                );
            }
            return;
        };
        let Some(request) = Reflect::get(&wake_lock, &"request".into())
            .ok()
            .and_then(|f| f.dyn_into::<Function>().ok())
        else {
            return;
        };
        let Ok(promise) = request.call1(&wake_lock, &"screen".into()) else {
            return;
        };
        let Ok(promise) = promise.dyn_into::<Promise>() else {
            return;
        };
        state.borrow_mut().requesting = true;
        let state = Rc::clone(state);
        wasm_bindgen_futures::spawn_local(async move {
            let result = wasm_bindgen_futures::JsFuture::from(promise).await;
            let mut state_mut = state.borrow_mut();
            state_mut.requesting = false;
            match result {
                Ok(sentinel) => {
                    state_mut.sentinel = Some(sentinel);
                    // Released between request and grant: let it go again.
                    if !state_mut.wanted {
                        drop(state_mut);
                        Self::release(&state);
                    }
                }
                Err(err) => {
                    // Typically the page isn't visible or the policy
                    // disallows it; the visibility listener retries.
                    web_sys::console::warn_2(&"KeyInSight: wake lock request failed".into(), &err);
                }
            }
        });
    }

    fn release(state: &Rc<RefCell<WakeState>>) {
        let Some(sentinel) = state.borrow_mut().sentinel.take() else {
            return;
        };
        if let Some(release) = Reflect::get(&sentinel, &"release".into())
            .ok()
            .and_then(|f| f.dyn_into::<Function>().ok())
        {
            // The returned promise resolves on its own; nothing to await.
            let _ = release.call0(&sentinel);
        }
    }
}
