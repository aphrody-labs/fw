#[path = "support/harness.rs"]
#[allow(dead_code)]
mod harness;

use std::{process::Command, time::{Duration, Instant}};

use harness::{Compositor, Session};

#[test]
fn toplevel_with_red_shm_buffer_shows_in_screenshot() {
    let compositor = Compositor::start("640x360", &[]);
    let mut session = Session::connect(&compositor);
    let window = session.toplevel("fw test", "fw.test", false);
    session.attach_solid(&window, 64, 64, 0xFFFF_0000, None);

    let windows: serde_json::Value = serde_json::from_str(&compositor.control("windows")).expect("windows json");
    assert_eq!(windows.as_array().map(Vec::len), Some(1), "{windows}");
    let entry = &windows[0];
    assert_eq!(entry["app_id"], "fw.test");
    assert_eq!(entry["title"], "fw test");
    assert_eq!(entry["w"], 64);
    assert_eq!(entry["h"], 64);
    assert_eq!(entry["mapped"], true);
    let (x, y) = (entry["x"].as_u64().expect("x") as usize, entry["y"].as_u64().expect("y") as usize);

    let shot = compositor.screenshot("ok 640x360 windows=1");
    assert_eq!((shot.width, shot.height), (640, 360));
    assert_eq!(shot.pixel(x + 32, y + 32), [255, 0, 0, 255]);
    assert_eq!(shot.pixel(x + 63, y + 63), [255, 0, 0, 255]);
    assert_ne!(shot.pixel(x + 100, y + 100), [255, 0, 0, 255]);
    assert_eq!(shot.pixel(639, 359), [0x20, 0x21, 0x24, 255]);

    assert_eq!(compositor.control("pointer 10 10 none"), "ok");
    assert_eq!(compositor.control(&format!("pointer {} {} left", x + 5, y + 5)), "ok");
    assert_eq!(compositor.control(&format!("pointer {} {} none", x + 5, y + 5)), "ok");
    assert_eq!(compositor.control("key 30 down"), "ok");
    assert_eq!(compositor.control("key 30 up"), "ok");
    assert!(compositor.control("bogus").starts_with("err"));
    drop(window.toplevel);
    drop(window.xdg_surface);
}

#[test]
fn maximized_toplevel_fills_canvas_through_viewport_and_sigterm_cleans_up() {
    let mut compositor = Compositor::start("640x360", &[]);
    let mut session = Session::connect(&compositor);
    let window = session.toplevel("full", "fw.full", true);
    assert!(session.client.maximized, "maximized state missing from configure");
    assert_eq!(session.client.toplevel_size, (640, 360));
    session.attach_solid(&window, 1, 1, 0xFF00_80FF, Some((640, 360)));

    let windows: serde_json::Value = serde_json::from_str(&compositor.control("windows")).expect("windows json");
    assert_eq!((windows[0]["x"].as_i64(), windows[0]["y"].as_i64()), (Some(0), Some(0)));
    assert_eq!((windows[0]["w"].as_i64(), windows[0]["h"].as_i64()), (Some(640), Some(360)));

    let shot = compositor.screenshot("ok 640x360 windows=1");
    assert_eq!(shot.pixel(5, 5), [0x00, 0x80, 0xFF, 255]);
    assert_eq!(shot.pixel(320, 180), [0x00, 0x80, 0xFF, 255]);
    assert_eq!(shot.pixel(639, 359), [0x00, 0x80, 0xFF, 255]);

    let status = Command::new("kill").args(["-TERM", &compositor.child.id().to_string()]).status().expect("kill");
    assert!(status.success());
    let deadline = Instant::now() + Duration::from_secs(10);
    let exit = loop {
        if let Some(exit) = compositor.child.try_wait().expect("try_wait") {
            break exit;
        }
        assert!(Instant::now() < deadline, "compositor ignored SIGTERM");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(exit.success(), "unclean exit {exit:?}");
    assert!(!compositor.socket.exists(), "wayland socket left behind");
    assert!(!compositor.ctl.exists(), "control socket left behind");
}
