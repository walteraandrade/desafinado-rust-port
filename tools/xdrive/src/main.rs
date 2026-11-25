// Tiny XTEST driver for scripts/perf. It panics on any X error: a failed run should stop loudly. Reads commands from stdin, one per line:
//   sleep MS | key COMBO [REPEAT] [GAP_MS] | type GAP_MS TEXT | scroll up|down N GAP_MS | move X Y | click X Y
use std::{io::BufRead, thread::sleep, time::Duration};
use x11rb::{
    connection::Connection, protocol::xproto::*, protocol::xtest::ConnectionExt as _,
    rust_connection::RustConnection, wrapper::ConnectionExt as _,
};

struct D {
    c: RustConnection,
    root: u32,
    map: Vec<(u32, u8, bool)>,
}
const KP: u8 = 2;
const KR: u8 = 3;
const BP: u8 = 4;
const BR: u8 = 5;
const MN: u8 = 6;

impl D {
    fn ev(&self, t: u8, d: u8) {
        self.c
            .xtest_fake_input(t, d, 0, self.root, 0, 0, 0)
            .unwrap();
    }
    /// The topmost mapped window: the app's, since nothing else runs on this display.
    fn top_window(&self) -> u32 {
        let tree = self.c.query_tree(self.root).unwrap().reply().unwrap();
        *tree
            .children
            .iter()
            .rev()
            .find(|w| {
                self.c
                    .get_window_attributes(**w)
                    .unwrap()
                    .reply()
                    .map(|a| a.map_state == MapState::VIEWABLE)
                    .unwrap_or(false)
            })
            .expect("no mapped window")
    }
    fn flush(&self) {
        self.c.sync().unwrap();
    }
    fn code(&self, sym: u32) -> (u8, bool) {
        self.map
            .iter()
            .find(|m| m.0 == sym)
            .map(|m| (m.1, m.2))
            .unwrap_or_else(|| panic!("no keycode for {sym:#x}"))
    }
    fn sym(name: &str) -> u32 {
        match name {
            "ctrl" => 0xffe3,
            "shift" => 0xffe1,
            "alt" => 0xffe9,
            "enter" | "return" => 0xff0d,
            "esc" => 0xff1b,
            "tab" => 0xff09,
            "backspace" => 0xff08,
            "delete" => 0xffff,
            "space" => 0x20,
            "up" => 0xff52,
            "down" => 0xff54,
            "left" => 0xff51,
            "right" => 0xff53,
            "home" => 0xff50,
            "end" => 0xff57,
            "pageup" => 0xff55,
            "pagedown" => 0xff56,
            "f8" => 0xffc5,
            n if n.len() == 1 => n.chars().next().unwrap() as u32,
            n => panic!("unknown key {n}"),
        }
    }
    fn combo(&self, combo: &str) {
        let keys: Vec<(u8, bool)> = combo.split('+').map(|k| self.code(Self::sym(k))).collect();
        let shift = self.code(0xffe1).0;
        for (k, s) in &keys {
            if *s {
                self.ev(KP, shift);
            }
            self.ev(KP, *k);
        }
        for (k, s) in keys.iter().rev() {
            self.ev(KR, *k);
            if *s {
                self.ev(KR, shift);
            }
        }
        self.flush();
    }
    fn ch(&self, c: char) {
        let sym = match c {
            '\n' => 0xff0d,
            c => c as u32,
        };
        let (k, s) = self.code(sym);
        let shift = self.code(0xffe1).0;
        if s {
            self.ev(KP, shift);
        }
        self.ev(KP, k);
        self.ev(KR, k);
        if s {
            self.ev(KR, shift);
        }
        self.flush();
    }
}

fn main() {
    let (c, screen) = x11rb::connect(None).unwrap();
    let root = c.setup().roots[screen].root;
    let min = c.setup().min_keycode;
    let max = c.setup().max_keycode;
    let km = c
        .get_keyboard_mapping(min, max - min + 1)
        .unwrap()
        .reply()
        .unwrap();
    let per = km.keysyms_per_keycode as usize;
    let mut map = Vec::new();
    for (i, syms) in km.keysyms.chunks(per).enumerate() {
        let code = min + i as u8;
        if let Some(s) = syms.first() {
            if *s != 0 && !map.iter().any(|m: &(u32, u8, bool)| m.0 == *s) {
                map.push((*s, code, false));
            }
        }
        if let Some(s) = syms.get(1) {
            if *s != 0 && !map.iter().any(|m: &(u32, u8, bool)| m.0 == *s) {
                map.push((*s, code, true));
            }
        }
    }
    let d = D { c, root, map };
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let mut p = line.splitn(2, ' ');
        let cmd = p.next().unwrap_or("");
        let rest = p.next().unwrap_or("");
        let a: Vec<&str> = rest.split(' ').collect();
        match cmd {
            "sleep" => sleep(Duration::from_millis(a[0].parse().unwrap())),
            "key" => {
                let n: u32 = a.get(1).map_or(1, |v| v.parse().unwrap());
                let gap: u64 = a.get(2).map_or(0, |v| v.parse().unwrap());
                for _ in 0..n {
                    d.combo(a[0]);
                    sleep(Duration::from_millis(gap));
                }
            }
            "type" => {
                let (gap, text) = rest.split_once(' ').unwrap();
                let gap: u64 = gap.parse().unwrap();
                for c in text.replace("\\n", "\n").chars() {
                    d.ch(c);
                    sleep(Duration::from_millis(gap));
                }
            }
            "scroll" => {
                let b = if a[0] == "up" { 4 } else { 5 };
                let n: u32 = a[1].parse().unwrap();
                let gap: u64 = a[2].parse().unwrap();
                for _ in 0..n {
                    d.ev(BP, b);
                    d.ev(BR, b);
                    d.flush();
                    sleep(Duration::from_millis(gap));
                }
            }
            "move" => {
                d.c.xtest_fake_input(
                    MN,
                    0,
                    0,
                    d.root,
                    a[0].parse().unwrap(),
                    a[1].parse().unwrap(),
                    0,
                )
                .unwrap();
                d.flush();
            }
            "click" => {
                d.c.xtest_fake_input(
                    MN,
                    0,
                    0,
                    d.root,
                    a[0].parse().unwrap(),
                    a[1].parse().unwrap(),
                    0,
                )
                .unwrap();
                d.ev(BP, 1);
                d.ev(BR, 1);
                d.flush();
            }
            "focus" => {
                let win = d.top_window();
                d.c.set_input_focus(InputFocus::PARENT, win, x11rb::CURRENT_TIME)
                    .unwrap();
                d.c.configure_window(win, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))
                    .unwrap();
                let g = d.c.get_geometry(win).unwrap().reply().unwrap();
                eprintln!(
                    "focused {win:#x} at {},{} {}x{}",
                    g.x, g.y, g.width, g.height
                );
                d.flush();
            }
            "resize" => {
                let w: u32 = a[0].parse().unwrap();
                let h: u32 = a[1].parse().unwrap();
                let win = d.top_window();
                d.c.configure_window(win, &ConfigureWindowAux::new().width(w).height(h))
                    .unwrap();
                d.flush();
            }
            "" => {}
            other => panic!("unknown command {other}"),
        }
    }
}
