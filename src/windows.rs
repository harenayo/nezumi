use {
    crate::{
        config::Config,
        key::Key,
    },
    eyre::{
        bail,
        ensure,
        eyre,
        OptionExt as _,
        Result,
    },
    keymacro::defer,
    rustc_hash::FxHashMap,
    std::{
        cell::{
            Cell,
            OnceCell,
        },
        collections::hash_map::Entry,
        mem::size_of,
    },
    tracing::{
        debug,
        instrument,
    },
    windows::Win32::{
        Foundation::{
            LPARAM,
            LRESULT,
            WPARAM,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{
                MapVirtualKeyW,
                SendInput,
                INPUT,
                INPUT_0,
                INPUT_KEYBOARD,
                INPUT_MOUSE,
                KEYBDINPUT,
                KEYEVENTF_KEYUP,
                MAPVK_VK_TO_VSC,
                MOUSEEVENTF_LEFTDOWN,
                MOUSEEVENTF_LEFTUP,
                MOUSEEVENTF_MIDDLEDOWN,
                MOUSEEVENTF_MIDDLEUP,
                MOUSEEVENTF_RIGHTDOWN,
                MOUSEEVENTF_XDOWN,
                MOUSEEVENTF_XUP,
                MOUSEINPUT,
                MOUSE_EVENT_FLAGS,
                VIRTUAL_KEY,
                VK_0,
                VK_1,
                VK_2,
                VK_3,
                VK_4,
                VK_5,
                VK_6,
                VK_7,
                VK_8,
                VK_9,
                VK_A,
                VK_B,
                VK_BACK,
                VK_C,
                VK_CAPITAL,
                VK_D,
                VK_DELETE,
                VK_DOWN,
                VK_E,
                VK_END,
                VK_ESCAPE,
                VK_F,
                VK_F1,
                VK_F10,
                VK_F11,
                VK_F12,
                VK_F13,
                VK_F14,
                VK_F15,
                VK_F16,
                VK_F17,
                VK_F18,
                VK_F19,
                VK_F2,
                VK_F20,
                VK_F21,
                VK_F22,
                VK_F23,
                VK_F24,
                VK_F3,
                VK_F4,
                VK_F5,
                VK_F6,
                VK_F7,
                VK_F8,
                VK_F9,
                VK_G,
                VK_H,
                VK_HOME,
                VK_I,
                VK_INSERT,
                VK_J,
                VK_K,
                VK_L,
                VK_LBUTTON,
                VK_LCONTROL,
                VK_LEFT,
                VK_LMENU,
                VK_LSHIFT,
                VK_LWIN,
                VK_M,
                VK_MBUTTON,
                VK_N,
                VK_NEXT,
                VK_O,
                VK_OEM_1,
                VK_OEM_2,
                VK_OEM_3,
                VK_OEM_4,
                VK_OEM_5,
                VK_OEM_6,
                VK_OEM_7,
                VK_OEM_COMMA,
                VK_OEM_MINUS,
                VK_OEM_PERIOD,
                VK_OEM_PLUS,
                VK_P,
                VK_PAUSE,
                VK_PRIOR,
                VK_Q,
                VK_R,
                VK_RBUTTON,
                VK_RCONTROL,
                VK_RETURN,
                VK_RIGHT,
                VK_RMENU,
                VK_RSHIFT,
                VK_RWIN,
                VK_S,
                VK_SCROLL,
                VK_SNAPSHOT,
                VK_SPACE,
                VK_T,
                VK_TAB,
                VK_U,
                VK_UP,
                VK_V,
                VK_W,
                VK_X,
                VK_XBUTTON1,
                VK_XBUTTON2,
                VK_Y,
                VK_Z,
            },
            WindowsAndMessaging::{
                CallNextHookEx,
                PeekMessageW,
                SetWindowsHookExW,
                UnhookWindowsHookEx,
                HC_ACTION,
                KBDLLHOOKSTRUCT,
                LLKHF_INJECTED,
                LLMHF_INJECTED,
                MSG,
                MSLLHOOKSTRUCT,
                PM_REMOVE,
                WH_KEYBOARD_LL,
                WH_MOUSE_LL,
                WM_KEYDOWN,
                WM_KEYUP,
                WM_LBUTTONDOWN,
                WM_LBUTTONUP,
                WM_MBUTTONDOWN,
                WM_MBUTTONUP,
                WM_QUIT,
                WM_RBUTTONDOWN,
                WM_RBUTTONUP,
                WM_SYSKEYDOWN,
                WM_SYSKEYUP,
                WM_XBUTTONDOWN,
                WM_XBUTTONUP,
                XBUTTON1,
                XBUTTON2,
            },
        },
    },
};

type Keys = FxHashMap<u16, (Mode, Cell<State>, [INPUT; 2])>;
thread_local!(static KEYS: OnceCell<Keys> = const { OnceCell::new() });

#[instrument]
pub fn run(config: Config) -> Result<()> {
    let mut map = FxHashMap::default();

    for key in config.fast {
        let (key, inputs) = into_raw(key)?;
        map.insert(key.0, (Mode::Fast, Cell::new(State::Released), inputs));
    }

    {
        let (key, inputs) = into_raw(config.exit)?;

        match map.entry(key.0) {
            Entry::Occupied(_) => bail!("The exit key cannot be a fast key"),
            Entry::Vacant(entry) => entry.insert((Mode::Exit, Cell::new(State::Released), inputs)),
        };
    }

    KEYS.with(|cell| cell.set(map))
        .map_err(|_| eyre!("Failed to initialize"))?;

    let module = unsafe { GetModuleHandleW(Option::None) }?;

    let keyboard_hook =
        unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Option::Some(keyboard_proc), module, 0) }?;

    defer! {
        let _ = unsafe { UnhookWindowsHookEx(keyboard_hook) };
    }

    let mouse_hook =
        unsafe { SetWindowsHookExW(WH_MOUSE_LL, Option::Some(mouse_proc), module, 0) }?;

    defer! {
        let _ = unsafe { UnhookWindowsHookEx(mouse_hook) };
    }

    let mut message = MSG::default();

    loop {
        if unsafe { PeekMessageW(&mut message, Option::None, 0, 0, PM_REMOVE) }.as_bool()
            && message.message == WM_QUIT
        {
            break;
        }

        let exited = with_keys(|keys| {
            for (key, (mode, state, inputs)) in keys {
                let state = state.get();
                debug!("processing {key:X}, {mode:?}, {state:?}");

                if matches!(state, State::Released) {
                    continue;
                }

                match mode {
                    Mode::Fast => unsafe {
                        debug!("sending {key:X}");
                        SendInput(inputs, size_of::<INPUT>() as i32);
                    },
                    Mode::Exit => return true,
                }
            }

            false
        })?;

        if exited {
            break;
        }
    }

    Result::Ok(())
}

fn into_raw(key: Key) -> Result<(VIRTUAL_KEY, [INPUT; 2])> {
    Result::Ok(match key {
        Key::Backquote => convert_keyboard(VK_OEM_3)?,
        Key::Backslash => convert_keyboard(VK_OEM_5)?,
        Key::BracketLeft => convert_keyboard(VK_OEM_4)?,
        Key::BracketRight => convert_keyboard(VK_OEM_6)?,
        Key::Comma => convert_keyboard(VK_OEM_COMMA)?,
        Key::Zero => convert_keyboard(VK_0)?,
        Key::One => convert_keyboard(VK_1)?,
        Key::Two => convert_keyboard(VK_2)?,
        Key::Three => convert_keyboard(VK_3)?,
        Key::Four => convert_keyboard(VK_4)?,
        Key::Five => convert_keyboard(VK_5)?,
        Key::Six => convert_keyboard(VK_6)?,
        Key::Seven => convert_keyboard(VK_7)?,
        Key::Eight => convert_keyboard(VK_8)?,
        Key::Nine => convert_keyboard(VK_9)?,
        Key::Equal => convert_keyboard(VK_OEM_PLUS)?,
        Key::A => convert_keyboard(VK_A)?,
        Key::B => convert_keyboard(VK_B)?,
        Key::C => convert_keyboard(VK_C)?,
        Key::D => convert_keyboard(VK_D)?,
        Key::E => convert_keyboard(VK_E)?,
        Key::F => convert_keyboard(VK_F)?,
        Key::G => convert_keyboard(VK_G)?,
        Key::H => convert_keyboard(VK_H)?,
        Key::I => convert_keyboard(VK_I)?,
        Key::J => convert_keyboard(VK_J)?,
        Key::K => convert_keyboard(VK_K)?,
        Key::L => convert_keyboard(VK_L)?,
        Key::M => convert_keyboard(VK_M)?,
        Key::N => convert_keyboard(VK_N)?,
        Key::O => convert_keyboard(VK_O)?,
        Key::P => convert_keyboard(VK_P)?,
        Key::Q => convert_keyboard(VK_Q)?,
        Key::R => convert_keyboard(VK_R)?,
        Key::S => convert_keyboard(VK_S)?,
        Key::T => convert_keyboard(VK_T)?,
        Key::U => convert_keyboard(VK_U)?,
        Key::V => convert_keyboard(VK_V)?,
        Key::W => convert_keyboard(VK_W)?,
        Key::X => convert_keyboard(VK_X)?,
        Key::Y => convert_keyboard(VK_Y)?,
        Key::Z => convert_keyboard(VK_Z)?,
        Key::Minus => convert_keyboard(VK_OEM_MINUS)?,
        Key::Period => convert_keyboard(VK_OEM_PERIOD)?,
        Key::Quote => convert_keyboard(VK_OEM_7)?,
        Key::Semicolon => convert_keyboard(VK_OEM_1)?,
        Key::Slash => convert_keyboard(VK_OEM_2)?,
        Key::AltLeft => convert_keyboard(VK_LMENU)?,
        Key::AltRight => convert_keyboard(VK_RMENU)?,
        Key::Backspace => convert_keyboard(VK_BACK)?,
        Key::CapsLock => convert_keyboard(VK_CAPITAL)?,
        Key::ControlLeft => convert_keyboard(VK_LCONTROL)?,
        Key::ControlRight => convert_keyboard(VK_RCONTROL)?,
        Key::Enter => convert_keyboard(VK_RETURN)?,
        Key::SuperLeft => convert_keyboard(VK_LWIN)?,
        Key::SuperRight => convert_keyboard(VK_RWIN)?,
        Key::ShiftLeft => convert_keyboard(VK_LSHIFT)?,
        Key::ShiftRight => convert_keyboard(VK_RSHIFT)?,
        Key::Space => convert_keyboard(VK_SPACE)?,
        Key::Tab => convert_keyboard(VK_TAB)?,
        Key::Delete => convert_keyboard(VK_DELETE)?,
        Key::End => convert_keyboard(VK_END)?,
        Key::Home => convert_keyboard(VK_HOME)?,
        Key::Insert => convert_keyboard(VK_INSERT)?,
        Key::PageDown => convert_keyboard(VK_NEXT)?,
        Key::PageUp => convert_keyboard(VK_PRIOR)?,
        Key::ArrowDown => convert_keyboard(VK_DOWN)?,
        Key::ArrowLeft => convert_keyboard(VK_LEFT)?,
        Key::ArrowRight => convert_keyboard(VK_RIGHT)?,
        Key::ArrowUp => convert_keyboard(VK_UP)?,
        Key::Escape => convert_keyboard(VK_ESCAPE)?,
        Key::PrintScreen => convert_keyboard(VK_SNAPSHOT)?,
        Key::ScrollLock => convert_keyboard(VK_SCROLL)?,
        Key::Pause => convert_keyboard(VK_PAUSE)?,
        Key::F1 => convert_keyboard(VK_F1)?,
        Key::F2 => convert_keyboard(VK_F2)?,
        Key::F3 => convert_keyboard(VK_F3)?,
        Key::F4 => convert_keyboard(VK_F4)?,
        Key::F5 => convert_keyboard(VK_F5)?,
        Key::F6 => convert_keyboard(VK_F6)?,
        Key::F7 => convert_keyboard(VK_F7)?,
        Key::F8 => convert_keyboard(VK_F8)?,
        Key::F9 => convert_keyboard(VK_F9)?,
        Key::F10 => convert_keyboard(VK_F10)?,
        Key::F11 => convert_keyboard(VK_F11)?,
        Key::F12 => convert_keyboard(VK_F12)?,
        Key::F13 => convert_keyboard(VK_F13)?,
        Key::F14 => convert_keyboard(VK_F14)?,
        Key::F15 => convert_keyboard(VK_F15)?,
        Key::F16 => convert_keyboard(VK_F16)?,
        Key::F17 => convert_keyboard(VK_F17)?,
        Key::F18 => convert_keyboard(VK_F18)?,
        Key::F19 => convert_keyboard(VK_F19)?,
        Key::F20 => convert_keyboard(VK_F20)?,
        Key::F21 => convert_keyboard(VK_F21)?,
        Key::F22 => convert_keyboard(VK_F22)?,
        Key::F23 => convert_keyboard(VK_F23)?,
        Key::F24 => convert_keyboard(VK_F24)?,
        Key::MouseLeft => convert_mouse(
            VK_LBUTTON,
            MOUSEEVENTF_LEFTDOWN,
            MOUSEEVENTF_LEFTUP,
            Option::None,
        ),
        Key::MouseRight => convert_mouse(
            VK_RBUTTON,
            MOUSEEVENTF_RIGHTDOWN,
            MOUSEEVENTF_LEFTUP,
            Option::None,
        ),
        Key::MouseMiddle => convert_mouse(
            VK_MBUTTON,
            MOUSEEVENTF_MIDDLEDOWN,
            MOUSEEVENTF_MIDDLEUP,
            Option::None,
        ),
        Key::MouseBack => convert_mouse(
            VK_XBUTTON1,
            MOUSEEVENTF_XDOWN,
            MOUSEEVENTF_XUP,
            Option::Some(XBUTTON1),
        ),
        Key::MouseForward => convert_mouse(
            VK_XBUTTON2,
            MOUSEEVENTF_XDOWN,
            MOUSEEVENTF_XUP,
            Option::Some(XBUTTON2),
        ),
    })
}

fn convert_keyboard(key: VIRTUAL_KEY) -> Result<(VIRTUAL_KEY, [INPUT; 2])> {
    let code = unsafe { MapVirtualKeyW(key.0 as u32, MAPVK_VK_TO_VSC) };
    ensure!(code != 0, "Failed to get a scan code");

    let input = KEYBDINPUT {
        wVk: key,
        wScan: code as u16,
        ..Default::default()
    };

    Result::Ok((
        key,
        [
            input,
            KEYBDINPUT {
                dwFlags: KEYEVENTF_KEYUP,
                ..input
            },
        ]
        .map(|input| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: input,
            },
        }),
    ))
}

fn convert_mouse(
    key: VIRTUAL_KEY,
    down: MOUSE_EVENT_FLAGS,
    up: MOUSE_EVENT_FLAGS,
    x: Option<u16>,
) -> (VIRTUAL_KEY, [INPUT; 2]) {
    (
        key,
        [down, up].map(|kind| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    mouseData: x.unwrap_or(0) as u32,
                    dwFlags: kind,
                    ..Default::default()
                },
            },
        }),
    )
}

#[instrument]
unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    'proc: {
        if code != HC_ACTION as i32 {
            break 'proc;
        }

        let state = match wparam.0 as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => State::Pressed,
            WM_KEYUP | WM_SYSKEYUP => State::Released,
            _ => break 'proc,
        };

        let info = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };

        if (info.flags & LLKHF_INJECTED).0 != 0 {
            break 'proc;
        }

        let key = info.vkCode as u16;
        debug!("key: {key:X}, state: {state:?}");

        let _ = with_keys(|keys| {
            if let Option::Some((_, cell, _)) = keys.get(&key) {
                cell.set(state);
            }
        });
    }

    unsafe { CallNextHookEx(Option::None, code, wparam, lparam) }
}

#[instrument]
unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    'proc: {
        if code != HC_ACTION as i32 {
            break 'proc;
        }

        let info = unsafe { *(lparam.0 as *const MSLLHOOKSTRUCT) };

        let (key, state) = match (wparam.0 as u32, (info.mouseData >> 16) as u16) {
            (WM_LBUTTONDOWN, _) => (VK_LBUTTON, State::Pressed),
            (WM_LBUTTONUP, _) => (VK_LBUTTON, State::Released),
            (WM_RBUTTONDOWN, _) => (VK_RBUTTON, State::Pressed),
            (WM_RBUTTONUP, _) => (VK_RBUTTON, State::Released),
            (WM_MBUTTONDOWN, _) => (VK_MBUTTON, State::Pressed),
            (WM_MBUTTONUP, _) => (VK_MBUTTON, State::Released),
            (WM_XBUTTONDOWN, XBUTTON1) => (VK_XBUTTON1, State::Pressed),
            (WM_XBUTTONUP, XBUTTON1) => (VK_XBUTTON1, State::Released),
            (WM_XBUTTONDOWN, XBUTTON2) => (VK_XBUTTON2, State::Pressed),
            (WM_XBUTTONUP, XBUTTON2) => (VK_XBUTTON2, State::Released),
            _ => break 'proc,
        };

        if info.flags & LLMHF_INJECTED != 0 {
            break 'proc;
        }

        let key = key.0;
        debug!("key: {key:X}, state: {state:?}");

        let _ = with_keys(|keys| {
            if let Option::Some((_, cell, _)) = keys.get(&key) {
                cell.set(state);
            }
        });
    }

    unsafe { CallNextHookEx(Option::None, code, wparam, lparam) }
}

fn with_keys<T>(f: impl FnOnce(&Keys) -> T) -> Result<T> {
    KEYS.with(|cell| {
        Result::Ok(f(cell
            .get()
            .ok_or_eyre("Failed to get the configuration")?))
    })
}

#[derive(Debug)]
enum Mode {
    Fast,
    Exit,
}

#[derive(Clone, Copy, Debug)]
enum State {
    Pressed,
    Released,
}
