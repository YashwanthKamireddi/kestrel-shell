//! rig-pointer move X Y | click X Y [right] | scroll X Y LINES
//! Drives a virtual pointer on the compositor in WAYLAND_DISPLAY (the test rig).

use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle, globals::GlobalListContents};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};
use wayland_client::protocol::wl_pointer::{Axis, ButtonState};

struct S;
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for S {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_seat::WlSeat, ()> for S {
    fn event(_: &mut Self, _: &wl_seat::WlSeat, _: wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ZwlrVirtualPointerManagerV1, ()> for S {
    fn event(_: &mut Self, _: &ZwlrVirtualPointerManagerV1, _: <ZwlrVirtualPointerManagerV1 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ZwlrVirtualPointerV1, ()> for S {
    fn event(_: &mut Self, _: &ZwlrVirtualPointerV1, _: <ZwlrVirtualPointerV1 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let num = |i: usize| a.get(i).and_then(|v| v.parse::<u32>().ok()).expect("number");
    let conn = Connection::connect_to_env().expect("wayland");
    let (globals, mut q) = wayland_client::globals::registry_queue_init::<S>(&conn).expect("registry");
    let qh = q.handle();
    let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=7, ()).expect("seat");
    let mgr: ZwlrVirtualPointerManagerV1 = globals.bind(&qh, 1..=2, ()).expect("virtual pointer");
    let p = mgr.create_virtual_pointer(Some(&seat), &qh, ());
    let (w, h) = (1920, 1080);
    let t = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u32;
    let mv = |x: u32, y: u32| {
        p.motion_absolute(t(), x, y, w, h);
        p.frame();
    };
    match a[0].as_str() {
        "move" => mv(num(1), num(2)),
        "click" => {
            mv(num(1), num(2));
            let btn = if a.get(3).is_some_and(|s| s == "right") { 0x111 } else { 0x110 };
            conn.flush().ok();
            std::thread::sleep(std::time::Duration::from_millis(60));
            p.button(t(), btn, ButtonState::Pressed);
            p.frame();
            conn.flush().ok();
            std::thread::sleep(std::time::Duration::from_millis(60));
            p.button(t(), btn, ButtonState::Released);
            p.frame();
        }
        "drag" => {
            // Press at X1 Y1, move to X2 Y2 in steps, release.
            let (x1, y1, x2, y2) = (num(1) as f64, num(2) as f64, num(3) as f64, num(4) as f64);
            mv(x1 as u32, y1 as u32);
            conn.flush().ok();
            std::thread::sleep(std::time::Duration::from_millis(60));
            p.button(t(), 0x110, ButtonState::Pressed);
            p.frame();
            for i in 1..=12 {
                let k = f64::from(i) / 12.0;
                conn.flush().ok();
                std::thread::sleep(std::time::Duration::from_millis(30));
                mv((x1 + (x2 - x1) * k) as u32, (y1 + (y2 - y1) * k) as u32);
            }
            conn.flush().ok();
            std::thread::sleep(std::time::Duration::from_millis(60));
            p.button(t(), 0x110, ButtonState::Released);
            p.frame();
        }
        "scroll" => {
            mv(num(1), num(2));
            let lines: f64 = a[3].parse().unwrap();
            // As a mouse wheel: its notches (discrete steps), not touchpad pixels.
            p.axis_source(wayland_client::protocol::wl_pointer::AxisSource::Wheel);
            p.axis_discrete(t(), Axis::VerticalScroll, lines * 15.0, lines as i32);
            p.frame();
        }
        _ => eprintln!("rig-pointer move X Y | click X Y [right] | drag X1 Y1 X2 Y2 | scroll X Y LINES"),
    }
    q.roundtrip(&mut S).ok();
    std::thread::sleep(std::time::Duration::from_millis(50));
    p.destroy();
    q.roundtrip(&mut S).ok();
}
