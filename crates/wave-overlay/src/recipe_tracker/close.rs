//! Independent non-activating close targets; recipe graphics remain click-through.
use super::*;
use std::cell::{Cell, RefCell};
use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
pub(super) const SIZE: i32 = 28;
struct State {
    control: Arc<Control>,
    cursor: Rc<RefCell<cursor::GameCursor>>,
    id: RefCell<String>,
    pressed: RefCell<Option<String>>,
    position_changes: Cell<u64>,
    hovered: Cell<bool>,
}
pub(super) struct Button {
    window: HWND,
    state: Box<State>,
    anchor: Cell<HWND>,
    position: Cell<Option<[i32; 2]>>,
}
impl Drop for Button {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.window);
        }
    }
}
fn hover(window: HWND, state: &State, active: bool) {
    if state.hovered.replace(active) != active
        && let Err(error) = render::close(window, active)
    {
        eprintln!("Recipe close highlight unavailable: {error:#}");
    }
}
unsafe extern "system" fn procedure(window: HWND, message: u32, w: usize, l: isize) -> isize {
    if message == WM_NCCREATE {
        let create = unsafe { &*(l as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        return 1;
    }
    let pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const State;
    if !pointer.is_null() {
        // State is boxed and outlives its HWND; no mutable aliases reach this callback.
        let state = unsafe { &*pointer };
        match message {
            WM_WINDOWPOSCHANGING => {
                state.position_changes.set(state.position_changes.get().saturating_add(1))
            }
            WM_MOUSEMOVE => {
                if !state.hovered.get() {
                    let mut tracking = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: window,
                        dwHoverTime: 0,
                    };
                    unsafe {
                        TrackMouseEvent(&mut tracking);
                    }
                    hover(window, state, true);
                }
                return 0;
            }
            WM_MOUSELEAVE => {
                state.pressed.borrow_mut().take();
                hover(window, state, false);
                return 0;
            }
            WM_MOUSEACTIVATE => return MA_NOACTIVATE as isize,
            WM_SETCURSOR => {
                unsafe {
                    let cursor = state.cursor.borrow().handle();
                    SetCursor(cursor);
                }
                return 1;
            }
            WM_LBUTTONDOWN => {
                *state.pressed.borrow_mut() = Some(state.id.borrow().clone());
                return 0;
            }
            WM_LBUTTONUP => {
                let pressed = state.pressed.borrow_mut().take();
                let id = state.id.borrow().clone();
                let x = l as u16 as i16 as i32;
                let y = (l >> 16) as u16 as i16 as i32;
                if pressed.as_deref() == Some(id.as_str())
                    && (0..SIZE).contains(&x)
                    && (0..SIZE).contains(&y)
                {
                    state.control.remove(&id);
                }
                return 0;
            }
            WM_NCDESTROY => unsafe {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            },
            _ => {}
        }
    }
    unsafe { DefWindowProcW(window, message, w, l) }
}
impl Button {
    pub(super) fn new(
        control: Arc<Control>,
        cursor: Rc<RefCell<cursor::GameCursor>>,
    ) -> Result<Self> {
        let class = wide("StarRuptureRecipeClose");
        let module = unsafe { GetModuleHandleW(std::ptr::null()) };
        let wc = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
            hInstance: module,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        unsafe {
            RegisterClassW(&wc);
        }
        let state = Box::new(State {
            control,
            cursor,
            id: RefCell::new(String::new()),
            pressed: RefCell::new(None),
            position_changes: Cell::new(0),
            hovered: Cell::new(false),
        });
        let window = unsafe {
            CreateWindowExW(
                crate::stacking::EXTENDED_STYLE & !WS_EX_TRANSPARENT,
                class.as_ptr(),
                wide("Unpin recipe").as_ptr(),
                WS_POPUP,
                0,
                0,
                SIZE,
                SIZE,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                module,
                (&*state as *const State).cast(),
            )
        };
        ensure!(!window.is_null(), "Could not create recipe close button");
        let button = Self {
            window,
            state,
            anchor: Cell::new(std::ptr::null_mut()),
            position: Cell::new(None),
        };
        render::close(window, false)?;
        Ok(button)
    }
    pub(super) fn show(
        &self,
        game: HWND,
        anchor: HWND,
        id: &str,
        position: [i32; 2],
        visible: bool,
    ) {
        if !visible {
            self.hide();
            return;
        }
        if *self.state.id.borrow() != id {
            *self.state.pressed.borrow_mut() = None;
            *self.state.id.borrow_mut() = id.into();
        }
        if self.position.get() != Some(position) {
            hover(self.window, &self.state, false);
            unsafe {
                SetWindowPos(
                    self.window,
                    std::ptr::null_mut(),
                    position[0],
                    position[1],
                    SIZE,
                    SIZE,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            self.position.set(Some(position));
        }
        self.anchor.set(anchor);
        crate::stacking::follow_companion(self.window, anchor, game);
    }
    pub(super) fn hide(&self) {
        *self.state.pressed.borrow_mut() = None;
        unsafe {
            ShowWindow(self.window, SW_HIDE);
        }
        hover(self.window, &self.state, false);
    }
    pub(super) fn follow(&self, game: HWND) {
        if unsafe { IsWindowVisible(self.window) } != 0 {
            crate::stacking::follow_companion(self.window, self.anchor.get(), game);
        }
    }
}

pub(super) fn self_test() -> Result<()> {
    use starrupture_observation_model::recipe::{Ingredient, Snapshot};
    let foreground = unsafe { GetForegroundWindow() };
    let control = Arc::new(Control::default());
    for id in ["close-A", "close-B"] {
        control.toggle_at(Some([0, 0]));
        control.captured(
            control.request(),
            Some(Snapshot {
                recipe_id: id.into(),
                ingredients: vec![Ingredient {
                    item_id: "item".into(),
                    submitted: 0,
                    required: 10,
                    remaining: 10,
                    bag_count: Some(0),
                }],
                observed_at_ms: 1000,
                generation: 0,
                process: 1,
            }),
        );
    }
    let button =
        Button::new(control.clone(), Rc::new(RefCell::new(cursor::GameCursor::default())))?;
    struct Fixture(HWND);
    impl Drop for Fixture {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }
    let make = || -> Result<Fixture> {
        let window = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                wide("StarRuptureRecipeClose").as_ptr(),
                wide("Owned close-order fixture").as_ptr(),
                WS_POPUP,
                -32000,
                -32000,
                200,
                200,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        ensure!(!window.is_null(), "Close-order fixture create");
        unsafe {
            ShowWindow(window, SW_SHOWNOACTIVATE);
        }
        Ok(Fixture(window))
    };
    let game = make()?;
    let panel = make()?;
    ensure!(crate::stacking::follow_game(panel.0, game.0), "Recipe fixture stacking");
    button.show(game.0, panel.0, "close-A", [-32000, -32000], true);
    ensure!(
        unsafe { GetWindow(panel.0, GW_HWNDPREV) } == button.window,
        "Close target sits behind its panel"
    );
    unsafe {
        SendMessageW(button.window, WM_MOUSEMOVE, 0, 12 | (12 << 16));
    }
    ensure!(button.state.hovered.get(), "Close target did not highlight on mouse entry");
    let after_enter = button.state.position_changes.get();
    unsafe {
        SendMessageW(button.window, WM_MOUSEMOVE, 0, 13 | (12 << 16));
    }
    ensure!(
        button.state.position_changes.get() == after_enter,
        "Hover movement repeatedly redrew/reordered close target"
    );
    unsafe {
        SendMessageW(button.window, WM_MOUSELEAVE, 0, 0);
    }
    ensure!(!button.state.hovered.get(), "Close target did not clear highlight on mouse leave");
    button.state.position_changes.set(0);
    for _ in 0..20 {
        button.show(game.0, panel.0, "close-A", [-32000, -32000], true);
        button.follow(game.0);
    }
    ensure!(
        button.state.position_changes.get() == 0,
        "Stable close target moved/reordered on refresh"
    );
    ensure!(
        unsafe { GetWindow(panel.0, GW_HWNDPREV) } == button.window,
        "Follow put close target behind its panel"
    );
    button.hide();
    ensure!(!button.state.hovered.get(), "Hidden close target retained hover highlight");
    unsafe {
        let previous = GetCursor();
        let result = SendMessageW(button.window, WM_SETCURSOR, 0, 0);
        let arrow = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
        let actual = GetCursor();
        SetCursor(previous);
        ensure!(
            result == 1 && !arrow.is_null() && actual == arrow,
            "Close control did not select normal arrow cursor"
        );
    }
    *button.state.cursor.borrow_mut() = super::cursor::self_test()?;
    unsafe {
        let previous = GetCursor();
        let expected = button.state.cursor.borrow().handle();
        let result = SendMessageW(button.window, WM_SETCURSOR, 0, 0);
        let actual = GetCursor();
        SetCursor(previous);
        ensure!(
            result == 1
                && actual == expected
                && expected != LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            "Close control replaced the retained game-style cursor"
        );
    }
    *button.state.id.borrow_mut() = "close-A".into();
    unsafe {
        ensure!(
            SendMessageW(button.window, WM_MOUSEACTIVATE, 0, 0) == MA_NOACTIVATE as isize,
            "Close target would activate the overlay"
        );
        ensure!(
            GetWindowLongPtrW(button.window, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT == 0,
            "Close target is click-through"
        );
        SendMessageW(button.window, WM_LBUTTONDOWN, 0, 0);
        SendMessageW(button.window, WM_LBUTTONUP, 0, 12 | (12 << 16));
    }
    ensure!(
        control.views().len() == 1 && control.view().snapshot.unwrap().recipe_id == "close-B",
        "Close affected another pin"
    );
    *button.state.id.borrow_mut() = "close-B".into();
    unsafe {
        SendMessageW(button.window, WM_LBUTTONDOWN, 0, 0);
    }
    button.hide();
    ensure!(!button.state.hovered.get(), "Hidden close target retained hover highlight");
    unsafe {
        SendMessageW(button.window, WM_LBUTTONUP, 0, 12 | (12 << 16));
    }
    ensure!(control.views().len() == 1, "Hidden close target retained a pressed action");
    unsafe {
        SendMessageW(button.window, WM_LBUTTONDOWN, 0, 0);
        SendMessageW(button.window, WM_LBUTTONUP, 0, ((SIZE + 1) | (12 << 16)) as isize);
    }
    ensure!(control.views().len() == 1, "Outside release removed a pin");
    let window = button.window;
    drop(button);
    ensure!(unsafe { IsWindow(window) } == 0, "Close HWND leaked");
    ensure!(unsafe { GetForegroundWindow() } == foreground, "Close fixture changed focus");
    Ok(())
}
