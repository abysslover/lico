//! `lico capture` against a real X server (class L3, two tests, one private Xvfb each). Why so few: the display is the cost (a server, 8 MB paints, ImageMagick reads
//! of the root window), so a server serves every scenario of its test in turn and goes away with the test, also when it panics. The scenarios are
//! functions with their own labels, so a red names the scenario.
//!
//! The painter is not trusted: every full-screen pattern is read back with ImageMagick (`import -window root`) and compared with the pattern
//! before anything is captured. Every PNG the command writes is then read by the two readers of the PNG tests (the test's own decoder and the
//! outside programs) and compared with the pattern the screen is known to show; the agent copy is compared with an independent reference.

use super::tests::reference;
use super::*;
use crate::handoff::{mode_of, Scratch};
use crate::png::readers::{both_readers, outside, read_png};
use crate::png::tests::{noise, text_like};

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use x11rb::protocol::xproto::{ConnectionExt, CreateGCAux, CreateWindowAux, WindowClass};
use x11rb::rust_connection::RustConnection;

const SW: usize = 1920;
const SH: usize = 1080;

/// A private Xvfb on a display number the server chose itself (`-displayfd`); it is killed when this goes away.
struct Xvfb {
    child: Child,
    name: String,
}

impl Xvfb {
    fn start(geometry: &str) -> Xvfb {
        let mut child = Command::new("Xvfb")
            .args(["-displayfd", "1", "-screen", "0", geometry, "-nolisten", "tcp", "-noreset"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("Xvfb must be installed for these tests (they never skip)");
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        Xvfb { child, name: format!(":{}", line.trim()) }
    }
}

impl Drop for Xvfb {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The painter and the model of what the screen shows (RGB, SW x SH).
struct Painter {
    conn: RustConnection,
    root: u32,
    gc: u32,
    model: Vec<u8>,
}

impl Painter {
    fn new(display: &str) -> Painter {
        let (conn, num) = x11rb::connect(Some(display)).expect("the test display");
        let screen = &conn.setup().roots[num];
        let root = screen.root;
        // this painter writes 32 bit little endian pixels with the usual masks, and says so if the server is different
        let format = conn.setup().pixmap_formats.iter().find(|f| f.depth == 24).unwrap();
        assert_eq!((format.bits_per_pixel, screen.root_depth), (32, 24));
        let gc = conn.generate_id().unwrap();
        conn.create_gc(gc, root, &CreateGCAux::new()).unwrap();
        Painter { conn, root, gc, model: vec![0; SW * SH * 3] }
    }

    fn sync(&self) {
        self.conn.get_input_focus().unwrap().reply().unwrap();
    }

    /// Draws `rgb` (w x h) into `drawable` at x, y.
    fn put(&self, drawable: u32, x: i16, y: i16, w: usize, h: usize, rgb: &[u8]) {
        let rows = (200_000 / (w * 4)).max(1);
        let mut at = 0;
        while at < h {
            let n = rows.min(h - at);
            let mut data = Vec::with_capacity(n * w * 4);
            for p in rgb[at * w * 3..(at + n) * w * 3].chunks(3) {
                data.extend_from_slice(&[p[2], p[1], p[0], 0]);
            }
            self.conn.put_image(ImageFormat::Z_PIXMAP, drawable, self.gc, w as u16, n as u16, x, y + at as i16, 0, 24, &data).unwrap();
            at += n;
        }
        self.sync();
    }

    fn blit_model(&mut self, x: i64, y: i64, w: usize, h: usize, rgb: &[u8]) {
        for r in 0..h {
            for c in 0..w {
                let (sx, sy) = (x + c as i64, y + r as i64);
                if (0..SW as i64).contains(&sx) && (0..SH as i64).contains(&sy) {
                    let at = (sy as usize * SW + sx as usize) * 3;
                    self.model[at..at + 3].copy_from_slice(&rgb[(r * w + c) * 3..(r * w + c) * 3 + 3]);
                }
            }
        }
    }

    /// Takes every window off the screen again (the model never knew about the parts of the screen they covered).
    fn destroy_windows(&self) {
        for w in self.conn.query_tree(self.root).unwrap().reply().unwrap().children {
            self.conn.destroy_window(w).unwrap();
        }
        self.sync();
    }

    /// The whole screen becomes `pix`.
    fn paint_screen(&mut self, pix: &[u8]) {
        self.put(self.root, 0, 0, SW, SH, pix);
        self.model.copy_from_slice(pix);
    }

    /// Paints a rectangle of the root (and the model).
    fn paint_rect(&mut self, x: usize, y: usize, w: usize, h: usize, rgb: &[u8]) {
        self.put(self.root, x as i16, y as i16, w, h, rgb);
        self.blit_model(x as i64, y as i64, w, h, rgb);
    }

    /// A window of `w` x `h` inside `parent` at x, y (relative to the parent's inside), painted with `rgb`. `abs` is where its inside origin is on
    /// the screen, worked out by the caller. The model gets the part that is on the screen.
    fn window(&mut self, parent: u32, (x, y): (i16, i16), abs: (i64, i64), (w, h): (usize, usize), border: u16, rgb: &[u8], map: bool) -> u32 {
        let id = self.conn.generate_id().unwrap();
        self.conn
            .create_window(24, id, parent, x, y, w as u16, h as u16, border, WindowClass::INPUT_OUTPUT, 0, &CreateWindowAux::new().background_pixel(0).border_pixel(0x00FF_00FF))
            .unwrap();
        if map {
            self.conn.map_window(id).unwrap();
            self.put(id, 0, 0, w, h, rgb);
            self.blit_model(abs.0, abs.1, w, h, rgb);
        }
        self.sync();
        id
    }
}

fn crop(model: &[u8], x: usize, y: usize, w: usize, h: usize) -> Vec<u8> {
    (y..y + h).flat_map(|r| model[(r * SW + x) * 3..(r * SW + x + w) * 3].iter().copied()).collect()
}

/// ImageMagick's read of the whole root window, 8 bits a channel.
fn import_root(display: &str) -> Vec<u8> {
    let out = Command::new("import").args(["-display", display, "-window", "root", "-depth", "8", "rgb:-"]).output().expect("import (ImageMagick) must be installed");
    assert!(out.status.success(), "import: {}", String::from_utf8_lossy(&out.stderr));
    out.stdout
}

fn import_root_png_size(display: &str) -> usize {
    let out = Command::new("import").args(["-display", display, "-window", "root", "png:-"]).output().expect("import (ImageMagick) must be installed");
    assert!(out.status.success());
    out.stdout.len()
}

struct Fixture {
    display: String,
    dir: Scratch,
    count: std::cell::Cell<usize>,
}

struct Shot {
    answer: Answer,
    png: Vec<u8>,
    agent: Option<Vec<u8>>,
}

impl Fixture {
    fn opts(&self) -> Options {
        Options { display: Some(self.display.clone()), ..Options::default() }
    }

    fn next_out(&self) -> PathBuf {
        self.count.set(self.count.get() + 1);
        self.dir.0.join(format!("c{}.png", self.count.get()))
    }

    fn run(&self, kind: Kind, opts: &Options) -> Result<(Answer, PathBuf), CapError> {
        let out = self.next_out();
        run(&Request { kind, out: out.clone() }, opts).map(|a| (a, out))
    }

    fn shot(&self, kind: Kind, opts: &Options) -> Shot {
        let (answer, out) = self.run(kind.clone(), opts).unwrap_or_else(|e| panic!("{kind:?}: {e:?}"));
        let png = std::fs::read(&out).unwrap();
        let agent = match &answer.agent {
            Agent::File { path, .. } => Some(std::fs::read(path).unwrap()),
            _ => None,
        };
        Shot { answer, png, agent }
    }

    /// No temporary file and no file but the ones the captures made.
    fn no_leftovers(&self, label: &str) {
        for e in std::fs::read_dir(&self.dir.0).unwrap() {
            let name = e.unwrap().file_name().to_string_lossy().into_owned();
            assert!(name.starts_with('c') && name.ends_with(".png"), "{label}: a stray file {name}");
        }
    }
}

/// The points of the agent copy that are compared with the reference: all of a small copy, else the corners, the middle and a spread.
fn sample_points(w: usize, h: usize) -> Vec<(usize, usize)> {
    if w * h <= 400 {
        return (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).collect();
    }
    let mut v = vec![(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1), (w / 2, h / 2), (w / 3, h / 5)];
    v.extend((0..40).map(|i| ((i * 613 + 7) % w, (i * 271 + 3) % h)));
    v
}

/// The full file against the pixels the screen shows, through both readers; the agent copy through both readers and the reference.
fn verify(f: &Fixture, label: &str, shot: &Shot, want: &[u8], (w, h): (usize, usize), offset: (i64, i64)) {
    let a = &shot.answer;
    assert_eq!((a.width as usize, a.height as usize, a.offset), (w, h, offset), "{label}: the answer");
    assert_eq!(a.bytes as usize, shot.png.len(), "{label}: bytes");
    assert!(Path::new(&a.path).is_absolute(), "{label}");
    both_readers(&shot.png, w as u32, h as u32, want, label);
    assert_eq!(mode_of(Path::new(&a.path)), 0o600, "{label}: the mode of the file");
    let j = a.to_json();
    assert_eq!(j["ok"], json!(1));
    assert_eq!((j["width"].as_u64(), j["height"].as_u64(), j["bytes"].as_u64()), (Some(w as u64), Some(h as u64), Some(a.bytes)), "{label}");
    assert_eq!((j["offset"]["x"].as_i64(), j["offset"]["y"].as_i64()), (Some(offset.0), Some(offset.1)), "{label}");
    assert!(j["server"]["vendor"].as_str().is_some_and(|v| !v.is_empty()) && j["server"]["release"].as_u64().is_some(), "{label}: server");
    let (aw, ah) = shrink::agent_size(w as u32, h as u32, AGENT_MAX_SIDE);
    match (&a.agent, &shot.agent) {
        (Agent::Same, None) => {
            assert_eq!((aw as usize, ah as usize), (w, h), "{label}: an unshrunk capture has no second file");
            assert_eq!((j["agent"]["path"].as_str(), j["agent"]["scale"].as_f64()), (Some(a.path.as_str()), Some(1.0)), "{label}");
            assert!(!agent_path_of(Path::new(&a.path)).exists(), "{label}: a second file for an unshrunk capture");
        }
        (Agent::File { path, width, height, bytes, scale }, Some(png)) => {
            assert_eq!((*width, *height), (aw, ah), "{label}: agent size");
            assert!(aw.max(ah) <= AGENT_MAX_SIDE && aw.max(ah) == AGENT_MAX_SIDE.min(w.max(h) as u32), "{label}");
            assert_eq!(*bytes as usize, png.len(), "{label}");
            assert_ne!(path, &a.path, "{label}");
            assert_eq!(mode_of(Path::new(path)), 0o600, "{label}: the mode of the agent file");
            assert!((scale - aw.max(ah) as f64 / w.max(h) as f64).abs() < 1e-12 && *scale < 1.0, "{label}: scale");
            assert_eq!(j["agent"]["scale"].as_f64(), Some(*scale));
            // reader 1 and the outside programs agree on the agent copy, and it is the shrink of the pattern (sample points by the reference)
            let dec = read_png(png).unwrap_or_else(|e| panic!("{label}: agent: {e}"));
            assert_eq!((dec.w, dec.h), (aw, ah));
            let (ok, size, pixels, said) = outside(png, aw, ah);
            assert!(ok && size == format!("{aw} {ah}") && pixels == dec.pix, "{label}: agent copy through the outside readers: {said}");
            for (x, y, rgb) in reference(want, w, h, aw as usize, ah as usize, &sample_points(aw as usize, ah as usize)) {
                let at = (y * aw as usize + x) * 3;
                assert_eq!(&dec.pix[at..at + 3], &rgb, "{label}: agent pixel {x},{y}");
            }
        }
        (other, _) => panic!("{label}: unexpected agent {other:?}"),
    }
    f.no_leftovers(label);
}

fn screen_scenario(f: &Fixture, p: &mut Painter, label: &str, pix: &[u8]) -> Shot {
    p.paint_screen(pix);
    assert!(import_root(&f.display) == pix, "{label}: ImageMagick does not read the pattern the painter drew (the painter is wrong)");
    let t = std::time::Instant::now();
    let shot = f.shot(Kind::Screen, &f.opts());
    let took = t.elapsed();
    verify(f, label, &shot, pix, (SW, SH), (0, 0));
    let a = &shot.answer;
    let (ours, theirs) = (shot.png.len(), import_root_png_size(&f.display));
    eprintln!(
        "capture: {label}: full {} bytes (import -window root {} bytes, ratio {:.3}), agent {} bytes, {:?} for capture and both files",
        ours, theirs, ours as f64 / theirs as f64, shot.agent.as_ref().map_or(0, Vec::len), took
    );
    assert_eq!(a.width, 1920);
    assert!(matches!(&a.agent, Agent::File { width: 1568, height: 882, .. }), "{label}: {:?}", a.agent);
    shot
}

fn ratio(f: &Fixture, shot: &Shot) -> f64 {
    shot.png.len() as f64 / import_root_png_size(&f.display) as f64
}

/// 62 lines of source code in DejaVu Sans Mono 14 pt on white, drawn by ImageMagick: a real render, not a synthetic pattern.
fn source_render() -> Vec<u8> {
    let text: String = include_str!("../png/mod.rs").lines().take(62).map(|l| l.replace(['%', '\\'], "_")).collect::<Vec<_>>().join("\n");
    let out = Command::new("convert")
        .args(["-size", "1920x1080", "xc:white", "-font", "DejaVu-Sans-Mono", "-pointsize", "14", "-fill", "black", "-annotate", "+8+20"])
        .arg(&text)
        .args(["-depth", "8", "rgb:-"])
        .output()
        .expect("convert (ImageMagick) must be installed");
    assert!(out.status.success() && out.stdout.len() == SW * SH * 3, "convert: {}", String::from_utf8_lossy(&out.stderr));
    out.stdout
}

fn squares_scenario(f: &Fixture, p: &mut Painter) {
    // the click rule: the screen point of a click at (cx, cy) on the agent copy is offset + (cx, cy) / scale
    p.destroy_windows();
    p.paint_screen(&vec![0u8; SW * SH * 3]);
    let red: Vec<u8> = (0..64).flat_map(|_| [255u8, 0, 0]).collect();
    for (name, (rx, ry, rw, rh), shrunk) in [("shrunk region", (100usize, 40usize, 1700usize, 400usize), true), ("unshrunk region", (50, 60, 800, 600), false)] {
        let spots = [(10, 10), (rw - 18, 10), (10, rh - 18), (rw - 18, rh - 18), ((rw - 8) / 2, (rh - 8) / 2)];
        for (sx, sy) in spots {
            p.paint_rect(rx + sx, ry + sy, 8, 8, &red);
        }
        let shot = f.shot(Kind::Region { x: rx as i64, y: ry as i64, w: rw as i64, h: rh as i64 }, &f.opts());
        let want = crop(&p.model, rx, ry, rw, rh);
        verify(f, name, &shot, &want, (rw, rh), (rx as i64, ry as i64));
        let j = shot.answer.to_json();
        let (scale, ox, oy) = (j["agent"]["scale"].as_f64().unwrap(), j["offset"]["x"].as_f64().unwrap(), j["offset"]["y"].as_f64().unwrap());
        assert_eq!(scale < 1.0, shrunk, "{name}");
        let (pix, aw, ah) = match &shot.agent {
            Some(png) => {
                let d = read_png(png).unwrap();
                (d.pix, d.w as usize, d.h as usize)
            }
            None => (want.clone(), rw, rh),
        };
        // the centre of the red in each quarter (and the middle), in agent pixel edges
        let windows = [(0, 0, aw / 4, ah / 4), (3 * aw / 4, 0, aw, ah / 4), (0, 3 * ah / 4, aw / 4, ah), (3 * aw / 4, 3 * ah / 4, aw, ah), (3 * aw / 8, 3 * ah / 8, 5 * aw / 8, 5 * ah / 8)];
        for ((x0, y0, x1, y1), (sx, sy)) in windows.into_iter().zip(spots) {
            let (mut sum, mut cx, mut cy) = (0.0, 0.0, 0.0);
            for y in y0..y1 {
                for x in x0..x1 {
                    let r = pix[(y * aw + x) * 3] as f64;
                    sum += r;
                    cx += r * (x as f64 + 0.5);
                    cy += r * (y as f64 + 0.5);
                }
            }
            assert!(sum > 0.0, "{name}: no red in the window {x0},{y0}");
            let (px, py) = (ox + cx / sum / scale, oy + cy / sum / scale);
            let (ex, ey) = ((rx + sx) as f64 + 4.0, (ry + sy) as f64 + 4.0);
            assert!((px - ex).abs() <= 1.0 && (py - ey).abs() <= 1.0, "{name}: the click rule gives {px:.2},{py:.2}, the square is at {ex},{ey}");
            if shrunk && sx > 100 {
                // without the scale the rule would miss by far more than a pixel: the two kinds of region really differ
                assert!((ox + cx / sum - ex).abs() > 5.0, "{name}: the fixture does not tell the scale from 1");
            }
        }
    }
}

fn region_scenario(f: &Fixture, p: &Painter) {
    // edges of the region: 1 x 1, 1 x N, N x 1, odd widths, against the corners and the edges of the screen; x + w == screen width is accepted
    let cases: [(&str, i64, i64, i64, i64); 10] = [
        ("1x1", 5, 7, 1, 1),
        ("1x1 in the last pixel", 1919, 1079, 1, 1),
        ("1xN", 300, 20, 1, 37),
        ("Nx1", 17, 500, 53, 1),
        ("odd 101x33", 1, 1, 101, 33),
        ("odd 7x9", 1913, 1071, 7, 9),
        ("x+w == width", 1800, 10, 120, 50),
        ("y+h == height", 10, 1000, 80, 80),
        ("long thin, shrunk", 0, 100, 1920, 2),
        ("tall thin, shrunk", 400, 0, 3, 1080),
    ];
    for (label, x, y, w, h) in cases {
        let shot = f.shot(Kind::Region { x, y, w, h }, &f.opts());
        let want = crop(&p.model, x as usize, y as usize, w as usize, h as usize);
        verify(f, label, &shot, &want, (w as usize, h as usize), (x, y));
    }
    // refused: one past the screen, nothing in it, negative, not a number
    let bad: [(i64, i64, i64, i64); 8] = [(1, 0, 1920, 10), (0, 1, 10, 1080), (0, 0, 1921, 1), (0, 0, 1, 1081), (5, 5, 0, 5), (5, 5, 5, 0), (-1, 0, 10, 10), (0, 0, i64::MAX, 1)];
    for (x, y, w, h) in bad {
        let e = f.run(Kind::Region { x, y, w, h }, &f.opts()).unwrap_err();
        assert_eq!(e.reason, "bad_region", "{x},{y},{w},{h}");
    }
    f.no_leftovers("regions");
}

fn bands_scenario(f: &Fixture, p: &Painter) {
    // the same capture with other band heights is the same file: a row repeated or lost at a band edge would change it
    let kind = Kind::Region { x: 33, y: 41, w: 301, h: 197 };
    let want = crop(&p.model, 33, 41, 301, 197);
    let base = f.shot(kind.clone(), &f.opts());
    verify(f, "bands: default", &base, &want, (301, 197), (33, 41));
    for rows in [1usize, 2, 3, 7, 64, 196, 197, 198, 1000] {
        let opts = Options { band_bytes: rows * 301 * 4, ..f.opts() };
        let s = f.shot(kind.clone(), &opts);
        assert_eq!(s.png, base.png, "{rows} rows a band");
        verify(f, &format!("bands of {rows}"), &s, &want, (301, 197), (33, 41));
    }
    // a band smaller than a row is one row
    let s = f.shot(kind, &Options { band_bytes: 1, ..f.opts() });
    assert_eq!(s.png, base.png);
    // and a shrunk capture, whose shrinker crosses the band edges
    let wide = Kind::Region { x: 50, y: 60, w: 1700, h: 150 };
    let big = f.shot(wide.clone(), &f.opts());
    let odd = f.shot(wide, &Options { band_bytes: 7 * 1700 * 4 + 5, ..f.opts() });
    assert!(matches!(big.answer.agent, Agent::File { .. }), "the wide region is shrunk");
    assert_eq!((&big.png, &big.agent), (&odd.png, &odd.agent));
}

#[allow(clippy::too_many_arguments)]
fn windows_scenario(f: &Fixture, p: &mut Painter) {
    let root = p.root;
    let tile = |w: usize, h: usize, seed: u64| noise(w, h, seed);
    // a window inside a window: its own x, y are relative to the parent, the screen position is the sum
    let parent = p.window(root, (100, 50), (100, 50), (400, 300), 0, &tile(400, 300, 1), true);
    let child_pix = tile(120, 90, 2);
    let child = p.window(parent, (30, 20), (130, 70), (120, 90), 0, &child_pix, true);
    let shot = f.shot(Kind::Window(child), &f.opts());
    verify(f, "a window inside a window", &shot, &child_pix, (120, 90), (130, 70));
    // the parent shows its child's pixels (what is drawn over a window is in the picture, D4)
    let shot = f.shot(Kind::Window(parent), &f.opts());
    verify(f, "the parent with its child on top", &shot, &crop(&p.model, 100, 50, 400, 300), (400, 300), (100, 50));
    // a border: the picture is the inside, whose origin is the border's width further in
    let bpix = tile(200, 150, 3);
    let bordered = p.window(root, (700, 400), (705, 405), (200, 150), 5, &bpix, true);
    let shot = f.shot(Kind::Window(bordered), &f.opts());
    verify(f, "a window with a border", &shot, &bpix, (200, 150), (705, 405));
    // a decimal id and a hex id are the same window; the answer is the same
    // at the screen's edges: clipped, and the answer carries the clipped offset and size
    let right = p.window(root, (1850, 1040), (1850, 1040), (150, 100), 0, &tile(150, 100, 4), true);
    let shot = f.shot(Kind::Window(right), &f.opts());
    verify(f, "a window past the right and bottom edge", &shot, &crop(&p.model, 1850, 1040, 70, 40), (70, 40), (1850, 1040));
    let left = p.window(root, (-20, -10), (-20, -10), (100, 80), 0, &tile(100, 80, 5), true);
    let shot = f.shot(Kind::Window(left), &f.opts());
    verify(f, "a window past the left and top edge", &shot, &crop(&p.model, 0, 0, 80, 70), (80, 70), (0, 0));
    // covered: the window under another one shows the other's pixels
    let under = p.window(root, (1200, 600), (1200, 600), (80, 80), 0, &tile(80, 80, 6), true);
    let _over = p.window(root, (1230, 620), (1230, 620), (80, 80), 0, &tile(80, 80, 7), true);
    let shot = f.shot(Kind::Window(under), &f.opts());
    verify(f, "a covered window", &shot, &crop(&p.model, 1200, 600, 80, 80), (80, 80), (1200, 600));
    // refused
    let unmapped = p.window(root, (10, 10), (10, 10), (50, 50), 0, &[], false);
    let off = p.window(root, (2000, 10), (2000, 10), (50, 50), 0, &tile(50, 50, 8), true);
    for (label, id) in [("an id nobody has", 0x7FFF_FFF0u32), ("a window that is not mapped", unmapped), ("a window wholly off the screen", off)] {
        let e = f.run(Kind::Window(id), &f.opts()).unwrap_err();
        assert_eq!(e.reason, "no_such_window", "{label}: {e:?}");
    }
    f.no_leftovers("windows");
}

fn failure_scenario(f: &Fixture, p: &Painter) {
    // no display
    let e = run(&Request { kind: Kind::Screen, out: f.dir.0.join("never.png") }, &Options { display: Some(":4999".into()), ..Options::default() }).unwrap_err();
    assert_eq!(e.reason, "no_display", "{e:?}");
    // a folder that does not exist: write_failed, and nothing is made
    let missing = f.dir.0.join("no-such-folder/x.png");
    let e = run(&Request { kind: Kind::Region { x: 0, y: 0, w: 10, h: 10 }, out: missing }, &f.opts()).unwrap_err();
    assert_eq!(e.reason, "write_failed", "{e:?}");
    assert!(!f.dir.0.join("no-such-folder").exists());
    // the output name is a folder: the rename fails, and nothing is left, not the temporary file, not the agent copy of a shrunk capture
    let folder = f.dir.0.join("c-folder.png");
    std::fs::create_dir(&folder).unwrap();
    let e = run(&Request { kind: Kind::Region { x: 0, y: 0, w: 1700, h: 40 }, out: folder.clone() }, &f.opts()).unwrap_err();
    assert_eq!(e.reason, "write_failed", "{e:?}");
    assert!(std::fs::read_dir(&folder).unwrap().next().is_none());
    assert!(!agent_path_of(&folder).exists(), "an agent copy after a failed capture");
    std::fs::remove_dir(&folder).unwrap();
    f.no_leftovers("failures");
    // a link at the output name is replaced, never written through
    let victim = f.dir.0.join("victim.txt");
    std::fs::write(&victim, b"keep").unwrap();
    let out = f.dir.0.join("c900.png");
    std::os::unix::fs::symlink(&victim, &out).unwrap();
    run(&Request { kind: Kind::Region { x: 0, y: 0, w: 20, h: 20 }, out: out.clone() }, &f.opts()).unwrap();
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep");
    assert!(!std::fs::symlink_metadata(&out).unwrap().file_type().is_symlink());
    assert_eq!(read_png(&std::fs::read(&out).unwrap()).unwrap().pix, crop(&p.model, 0, 0, 20, 20));
    std::fs::remove_file(&victim).unwrap();
    // the mode does not depend on the umask people have
    extern "C" {
        fn umask(mask: u32) -> u32;
    }
    for mask in [0o077u32, 0o022] {
        let before = unsafe { umask(mask) };
        let out = f.next_out();
        let r = run(&Request { kind: Kind::Region { x: 0, y: 0, w: 5, h: 5 }, out: out.clone() }, &f.opts());
        unsafe { umask(before) };
        r.unwrap();
        assert_eq!(mode_of(&out), 0o600, "umask {mask:o}");
    }
}

fn too_large_scenario(f: &Fixture) {
    // the over-limit shape: the agent copy is not delivered and no file is left, the full capture is delivered; an old copy under the name goes
    let out = f.dir.0.join("c800.png");
    let stale = agent_path_of(&out);
    std::fs::write(&stale, b"from an earlier capture").unwrap();
    let a = run(&Request { kind: Kind::Screen, out: out.clone() }, &Options { agent_limit: 1000, ..f.opts() }).unwrap();
    assert!(matches!(a.agent, Agent::TooLarge { limit: 1000, bytes } if bytes > 1000), "{:?}", a.agent);
    assert!(!stale.exists(), "the agent file of a too large copy must not exist");
    let j = a.to_json();
    assert_eq!(j["agent"]["error"], json!("too_large"));
    assert_eq!(j["agent"]["limit"], json!(1000));
    assert!(j["agent"]["bytes"].as_u64().unwrap() > 1000 && j["agent"].get("path").is_none());
    assert!(read_png(&std::fs::read(&out).unwrap()).is_ok(), "the full capture is delivered");
    std::fs::remove_file(&out).unwrap();
    f.no_leftovers("too large");
}

fn start() -> (Xvfb, Painter, Fixture) {
    let server = Xvfb::start("1920x1080x24");
    let p = Painter::new(&server.name);
    let f = Fixture { display: server.name.clone(), dir: Scratch::new("capx"), count: std::cell::Cell::new(0) };
    (server, p, f)
}

#[test]
fn the_text_screen_and_the_noise_screen_on_a_real_x_server() {
    let (_server, mut p, f) = start();
    let text =screen_scenario(&f, &mut p, "text-like screen", &text_like(SW, SH));
    assert!(ratio(&f, &text) <= RATIO_TEXT_SCREEN, "text-like ratio {}", ratio(&f, &text));
    let noisy = screen_scenario(&f, &mut p, "noise screen", &noise(SW, SH, 99));
    let noise_agent = noisy.answer.agent.clone();
    assert!(ratio(&f, &noisy) <= 1.001 + 64.0 / noisy.png.len() as f64, "noise ratio {}", ratio(&f, &noisy));
    // the worst case: noise at 1920 x 1080, shrunk to 1568 x 882, is under the real limit and is delivered
    assert!(matches!(noise_agent, Agent::File { bytes, .. } if fits(bytes, AGENT_LIMIT)), "{noise_agent:?}");
    too_large_scenario(&f);
}

#[test]
fn a_real_render_regions_bands_windows_failures_and_the_click_rule_on_a_real_x_server() {
    let (_server, mut p, f) = start();
    let render = screen_scenario(&f, &mut p, "source code render", &source_render());
    let r = ratio(&f, &render);
    assert!(r <= RATIO_RENDER, "a real render costs {r:.3} times import");
    region_scenario(&f, &p);
    bands_scenario(&f, &p);
    windows_scenario(&f, &mut p);
    failure_scenario(&f, &p);
    squares_scenario(&f, &mut p);
}

/// The ceiling for a real render against `import -window root` (the stop level of the lead is 3; mail 119 point 0).
const RATIO_RENDER: f64 = 3.0;

#[test]
fn a_screen_of_another_depth_is_unsupported() {
    let server = Xvfb::start("640x480x16");
    let out = Scratch::new("capx16").0.join("x.png");
    let e = run(&Request { kind: Kind::Screen, out: out.clone() }, &Options { display: Some(server.name.clone()), ..Options::default() }).unwrap_err();
    assert_eq!(e.reason, "unsupported_screen", "{e:?}");
    assert!(!out.exists());
}

/// The ceiling for the synthetic text-like screen against `import -window root` (measured 31.55 on 2026-10-29; the S02a ceiling `RATIO_TEXT` of 30 was set
/// against `convert` of raw pixels, which is a little better than `import` on a screen). The synthetic screen is a flat pattern that ImageMagick packs
/// into 1.4 KB, so the ratio says little; the real render below is the figure that counts. The lead accepted fixed Huffman codes (mail 119).
const RATIO_TEXT_SCREEN: f64 = 40.0;
