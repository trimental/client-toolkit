extern crate byteorder;
extern crate smithay_client_toolkit as sctk;
extern crate tempfile;
extern crate wayland_client;

use std::cmp::min;
use std::io::Write;
use std::os::unix::io::AsRawFd;

use byteorder::{NativeEndian, WriteBytesExt};

use wayland_client::{Display, GlobalManager, Proxy};
use wayland_client::protocol::{wl_compositor, wl_seat, wl_shell, wl_shell_surface, wl_shm};
use wayland_client::protocol::wl_display::RequestsTrait as DisplayRequests;
use wayland_client::protocol::wl_compositor::RequestsTrait as CompositorRequests;
use wayland_client::protocol::wl_surface::RequestsTrait as SurfaceRequests;
use wayland_client::protocol::wl_seat::RequestsTrait as SeatRequests;
use wayland_client::protocol::wl_shm::RequestsTrait as ShmRequests;
use wayland_client::protocol::wl_shm_pool::RequestsTrait as PoolRequests;
use wayland_client::protocol::wl_shell::RequestsTrait as ShellRequests;
use wayland_client::protocol::wl_shell_surface::RequestsTrait as ShellSurfaceRequests;

use sctk::keyboard::{map_keyboard_auto, Event as KbEvent};

fn main() {
    let (display, mut event_queue) = Display::connect_to_env().unwrap();
    let globals = GlobalManager::new(display.get_registry().unwrap());

    // roundtrip to retrieve the globals list
    event_queue.sync_roundtrip().unwrap();

    /*
     * Create a buffer with window contents
     */

    // buffer (and window) width and height
    let buf_x: u32 = 320;
    let buf_y: u32 = 240;

    // create a tempfile to write the conents of the window on
    let mut tmp = tempfile::tempfile()
        .ok()
        .expect("Unable to create a tempfile.");
    // write the contents to it, lets put a nice color gradient
    for i in 0..(buf_x * buf_y) {
        let x = (i % buf_x) as u32;
        let y = (i / buf_x) as u32;
        let r: u32 = min(((buf_x - x) * 0xFF) / buf_x, ((buf_y - y) * 0xFF) / buf_y);
        let g: u32 = min((x * 0xFF) / buf_x, ((buf_y - y) * 0xFF) / buf_y);
        let b: u32 = min(((buf_x - x) * 0xFF) / buf_x, (y * 0xFF) / buf_y);
        let _ = tmp.write_u32::<NativeEndian>((0xFF << 24) + (r << 16) + (g << 8) + b);
    }
    let _ = tmp.flush();

    /*
     * Init wayland objects
     */

    // The compositor allows us to creates surfaces
    let compositor = globals
        .instanciate::<wl_compositor::WlCompositor>(1)
        .unwrap()
        .implement(|_, _| {});
    let surface = compositor.create_surface().unwrap().implement(|_, _| {});

    // Init the SHM to define buffers to display somehting
    let shm = globals
        .instanciate::<wl_shm::WlShm>(1)
        .unwrap()
        .implement(|_, _| {});
    let pool = shm.create_pool(tmp.as_raw_fd(), (buf_x * buf_y * 4) as i32)
        .unwrap()
        .implement(|_, _| {});
    let buffer = pool.create_buffer(
        0,
        buf_x as i32,
        buf_y as i32,
        (buf_x * 4) as i32,
        wl_shm::Format::Argb8888,
    ).unwrap()
        .implement(|_, _| {});

    // The shell allows us to define our surface as a "toplevel", to show a window
    //
    // NOTE: the wl_shell interface is actually deprecated in favour of the xdg_shell
    // protocol, available in wayland-protocols. But this will do for this example.
    let shell = globals
        .instanciate::<wl_shell::WlShell>(1)
        .unwrap()
        .implement(|_, _| {});
    let shell_surface = shell.get_shell_surface(&surface).unwrap().implement(
        |event, shell_surface: Proxy<wl_shell_surface::WlShellSurface>| {
            use wayland_client::protocol::wl_shell_surface::{Event, RequestsTrait};
            // This ping/pong mechanism is used by the wayland server to detect
            // unresponsive applications
            if let Event::Ping { serial } = event {
                shell_surface.pong(serial);
            }
        },
    );

    // Set our surface as toplevel and define its contents
    shell_surface.set_toplevel();
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();

    /*
     * Keyboard initialization
     */

    // initialize a seat to retrieve keyboard events
    let seat = globals
        .instanciate::<wl_seat::WlSeat>(1)
        .unwrap()
        .implement(move |_, _| {});

    let _keyboard = map_keyboard_auto(seat.get_keyboard().unwrap(), move |event: KbEvent, _| {
        match event {
            KbEvent::Enter {
                modifiers, keysyms, ..
            } => {
                println!(
                    "Gained focus while {} keys pressed and modifiers are {:?}.",
                    keysyms.len(),
                    modifiers
                );
            }
            KbEvent::Leave { .. } => {
                println!("Lost focus.");
            }
            KbEvent::Key {
                keysym,
                state,
                utf8,
                modifiers,
                ..
            } => {
                println!("Key {:?}: {:x}.", state, keysym);
                println!(" -> Modifers are {:?}", modifiers);
                if let Some(txt) = utf8 {
                    println!(" -> Received text \"{}\".", txt,);
                }
            }
            KbEvent::RepeatInfo { rate, delay } => {
                println!(
                "Received repeat info: start repeating every {}ms after an initial delay of {}ms",
                rate, delay
            );
            }
        }
    });

    loop {
        display.flush().unwrap();
        event_queue.dispatch().unwrap();
    }
}