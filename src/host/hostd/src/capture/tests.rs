//! Tests of the parts of `lico capture` that need no X server: the pixel layouts, the shrink against an independent reference, the size of the
//! agent copy, the guard, the arguments, the mapping of X errors and the output files. The tests with a server are in `tests_x.rs`.

use super::layout::{Layout, Parts};
use super::shrink::{agent_size, Shrinker};
use super::*;
use crate::handoff::{mode_of, Scratch};
use crate::png::tests::{noise, text_like};
use std::process::Command;

// ---- layouts ---------------------------------------------------------------------------------------------------------------

fn parts(depth: u8, bpp: u8, pad: u8, msb: bool, masks: [u32; 3]) -> Parts {
    Parts { depth, bits_per_pixel: bpp, scanline_pad: pad, msb_first: msb, true_color: true, red_mask: masks[0], green_mask: masks[1], blue_mask: masks[2] }
}

/// The bytes of one pixel the way a server of this layout sends it, built here from the number, not by the code under test.
fn pixel_bytes(rgb: [u8; 3], shifts: [u32; 3], bpp: usize, msb: bool, junk: u8) -> Vec<u8> {
    let mut v: u32 = if bpp == 4 { (junk as u32) << (0..32u32).step_by(8).find(|s| !shifts.contains(s)).unwrap() } else { 0 };
    for c in 0..3 {
        v |= (rgb[c] as u32) << shifts[c];
    }
    let le: Vec<u8> = (0..bpp).map(|i| (v >> (8 * i)) as u8).collect();
    if msb {
        le.into_iter().rev().collect()
    } else {
        le
    }
}

#[test]
fn every_accepted_layout_is_converted_and_the_scanline_padding_is_skipped() {
    let rgb = [[1u8, 2, 3], [250, 128, 7], [0, 255, 0], [9, 9, 200], [255, 255, 255]];
    // (name, depth, bits a pixel, pad, msb, masks, shifts r g b)
    let cases: [(&str, u8, u8, u8, bool, [u32; 3], [u32; 3]); 7] = [
        ("32 bit little endian, the usual", 24, 32, 32, false, [0xFF0000, 0xFF00, 0xFF], [16, 8, 0]),
        ("32 bit big endian", 24, 32, 32, true, [0xFF0000, 0xFF00, 0xFF], [16, 8, 0]),
        ("24 bit little endian", 24, 24, 32, false, [0xFF0000, 0xFF00, 0xFF], [16, 8, 0]),
        ("24 bit big endian, pad 8", 24, 24, 8, true, [0xFF0000, 0xFF00, 0xFF], [16, 8, 0]),
        ("32 bit with the masks the other way round (BGR)", 24, 32, 32, false, [0xFF, 0xFF00, 0xFF0000], [0, 8, 16]),
        ("depth 32, alpha in the high byte", 32, 32, 32, false, [0xFF0000, 0xFF00, 0xFF], [16, 8, 0]),
        ("24 bit with the pad of 64", 24, 24, 64, false, [0xFF0000, 0xFF00, 0xFF], [16, 8, 0]),
    ];
    for (name, depth, bpp, pad, msb, masks, shifts) in cases {
        let layout = Layout::new(&parts(depth, bpp, pad, msb, masks)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let width = rgb.len();
        let stride = layout.stride(width);
        let n = bpp as usize / 8;
        let expected_stride = (width * bpp as usize).div_ceil(pad as usize) * pad as usize / 8;
        assert_eq!(stride, expected_stride, "{name}: stride");
        assert!(stride >= width * n, "{name}");
        // two rows, the second one the first reversed, the padding filled with a marker that must never reach the output
        let mut data = vec![0xEEu8; stride * 2];
        for (r, order) in [(0usize, (0..width).collect::<Vec<_>>()), (1, (0..width).rev().collect())] {
            for (x, &src) in order.iter().enumerate() {
                let px = pixel_bytes(rgb[src], shifts, n, msb, 0xAB);
                data[r * stride + x * n..r * stride + (x + 1) * n].copy_from_slice(&px);
            }
        }
        for (r, order) in [(0usize, (0..width).collect::<Vec<_>>()), (1, (0..width).rev().collect())] {
            let mut out = vec![0u8; width * 3];
            layout.row_to_rgb(&data[r * stride..], width, &mut out);
            let want: Vec<u8> = order.iter().flat_map(|&i| rgb[i]).collect();
            assert_eq!(out, want, "{name}: row {r}");
        }
    }
}

#[test]
fn the_stride_of_the_usual_screen_and_of_odd_widths() {
    let usual = Layout::new(&parts(24, 32, 32, false, [0xFF0000, 0xFF00, 0xFF])).unwrap();
    assert_eq!(usual.stride(1920), 7680);
    assert_eq!(usual.stride(1), 4);
    let packed = Layout::new(&parts(24, 24, 32, false, [0xFF0000, 0xFF00, 0xFF])).unwrap();
    assert_eq!((packed.stride(1), packed.stride(5), packed.stride(1920)), (4, 16, 5760));
    let tight = Layout::new(&parts(24, 24, 8, false, [0xFF0000, 0xFF00, 0xFF])).unwrap();
    assert_eq!(tight.stride(5), 15);
}

#[test]
fn a_layout_that_is_not_handled_is_refused_with_a_reason() {
    let ok = parts(24, 32, 32, false, [0xFF0000, 0xFF00, 0xFF]);
    assert!(Layout::new(&ok).is_ok());
    let refused: [(&str, Parts); 8] = [
        ("not TrueColor", Parts { true_color: false, ..ok }),
        ("depth 16", parts(16, 16, 16, false, [0xF800, 0x7E0, 0x1F])),
        ("depth 8", parts(8, 8, 8, false, [0, 0, 0])),
        ("16 bits a pixel at depth 24", Parts { bits_per_pixel: 16, ..ok }),
        ("masks of 565", Parts { red_mask: 0xF800, green_mask: 0x7E0, blue_mask: 0x1F, ..ok }),
        ("two masks the same", Parts { green_mask: 0xFF0000, ..ok }),
        ("a mask outside a 24 bit pixel", Parts { bits_per_pixel: 24, red_mask: 0xFF00_0000, ..ok }),
        ("a pad of 12", Parts { scanline_pad: 12, ..ok }),
    ];
    for (name, p) in refused {
        assert!(Layout::new(&p).is_err(), "{name} must be refused");
    }
}

// ---- the size of the agent copy and the guard ------------------------------------------------------------------------------

#[test]
fn the_agent_copy_is_at_most_1568_on_the_long_side() {
    let table: [((u32, u32), (u32, u32)); 9] = [
        ((1920, 1080), (1568, 882)),
        ((1568, 1000), (1568, 1000)),
        ((1569, 1), (1568, 1)),
        ((1080, 1920), (882, 1568)),
        ((3, 4000), (1, 1568)),
        ((1921, 1080), (1568, 882)), // 881.5 rounds up
        ((2560, 1440), (1568, 882)),
        ((1, 1), (1, 1)),
        ((3840, 2160), (1568, 882)),
    ];
    for ((w, h), want) in table {
        assert_eq!(agent_size(w, h, 1568), want, "{w} x {h}");
    }
    assert_eq!(agent_size(100, 100, 50), (50, 50));
    assert_eq!(agent_size(100, 30, 50), (50, 15));
}

#[test]
fn the_size_guard_is_a_function_of_the_limit_and_works_on_both_sides_of_it() {
    assert!(fits(0, 10) && fits(9, 10) && fits(10, 10));
    assert!(!fits(11, 10) && !fits(u64::MAX, 10));
    // the first-party API takes 10 MB of base64: three bytes of file for four of text
    assert_eq!(AGENT_LIMIT, 10_000_000 / 4 * 3);
    // the worst case, noise at 1568 x 882: raw size plus 0.062 % (the measured cost of the encoder on noise), plus a few hundred bytes
    let raw = 1568u64 * 882 * 3;
    assert!(fits(raw + raw / 1000 + 1000, AGENT_LIMIT), "the worst case is {raw} bytes of raw pixels");
}

// ---- the shrink against the independent reference --------------------------------------------------------------------------

const REF_PY: &str = include_str!("ref_shrink.py");

/// The destination pixels `points` (all when empty) of the shrink of `src`, by the Python reference (Fractions).
pub(super) fn reference(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize, points: &[(usize, usize)]) -> Vec<(usize, usize, [u8; 3])> {
    let dir = Scratch::new("capref");
    let (raw, script) = (dir.0.join("in.rgb"), dir.0.join("ref.py"));
    std::fs::write(&raw, src).unwrap();
    std::fs::write(&script, REF_PY).unwrap();
    let mut cmd = Command::new("python3");
    cmd.arg("-I").arg(&script).arg(&raw).args([sw, sh, dw, dh].map(|v| v.to_string()));
    cmd.args(points.iter().map(|(x, y)| format!("{x},{y}")));
    let out = cmd.output().expect("python3 must be installed for these tests (they never skip)");
    assert!(out.status.success(), "reference: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| {
            let v: Vec<usize> = l.split(' ').map(|n| n.parse().unwrap()).collect();
            (v[0], v[1], [v[2] as u8, v[3] as u8, v[4] as u8])
        })
        .collect()
}

/// The shrink by the code under test, fed row by row; the destination rows in a `Vec`.
pub(super) fn shrink_all(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    let mut s = Shrinker::new(sw, sh, dw, dh);
    let mut out = Vec::new();
    let mut rows = 0;
    for row in src.chunks(sw * 3) {
        s.push_row(row, &mut |r| {
            out.extend_from_slice(r);
            rows += 1;
            Ok(())
        })
        .unwrap();
    }
    assert!(s.done() && rows == dh, "{sw}x{sh} -> {dw}x{dh}: {rows} rows came out");
    out
}

fn compare_with_reference(name: &str, src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize, points: &[(usize, usize)]) {
    let ours = shrink_all(src, sw, sh, dw, dh);
    assert_eq!(ours.len(), dw * dh * 3, "{name}");
    let theirs = reference(src, sw, sh, dw, dh, points);
    assert_eq!(theirs.len(), if points.is_empty() { dw * dh } else { points.len() }, "{name}");
    for (x, y, rgb) in theirs {
        let at = (y * dw + x) * 3;
        assert_eq!(&ours[at..at + 3], &rgb, "{name}: destination pixel {x},{y}");
    }
}

#[test]
fn the_box_filter_equals_the_reference_on_small_pictures_every_pixel() {
    for (i, (sw, sh, dw, dh)) in [(7, 5, 3, 2), (4, 4, 2, 2), (5, 5, 5, 5), (10, 1, 3, 1), (1, 10, 1, 3), (13, 9, 8, 7), (9, 13, 7, 8), (3, 3, 1, 1), (11, 7, 10, 6)].into_iter().enumerate() {
        let src = noise(sw, sh, 100 + i as u64);
        compare_with_reference(&format!("noise {sw}x{sh}->{dw}x{dh}"), &src, sw, sh, dw, dh, &[]);
    }
    // values that make halves: 0 and 1 averaged over two pixels is 0.5, which rounds up
    let src = vec![0u8, 0, 0, 1, 1, 1];
    assert_eq!(shrink_all(&src, 2, 1, 1, 1), vec![1, 1, 1]);
    let src = vec![0u8, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0, 0];
    assert_eq!(shrink_all(&src, 2, 2, 1, 1), vec![1, 1, 1]); // 2/4 = 0.5 up
    let src = vec![0u8, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 0];
    assert_eq!(shrink_all(&src, 2, 2, 1, 1), vec![0, 0, 0]); // 1/4 = 0.25 down
}

#[test]
fn the_box_filter_equals_the_reference_on_1920_by_1080_at_the_edges_of_the_pattern() {
    let (sw, sh, dw, dh) = (1920usize, 1080usize, 1568usize, 882usize);
    let src = text_like(sw, sh); // blocks 6 x 10 in cells of 8 x 16: edges at x = 8k + 3 + 6 (y = 16k + 3 and 13) in the source
    let mut points = vec![(0, 0), (dw - 1, 0), (0, dh - 1), (dw - 1, dh - 1), (dw / 2, dh / 2)];
    // destination pixels whose source span contains the left or right edge of an ink block, and the same for rows
    for k in [0usize, 1, 2, 17, 100, 239] {
        for edge in [8 * k, 8 * k + 6] {
            let dx = edge * dw / sw;
            for y in [0usize, 5, 200, dh - 1] {
                points.push((dx.saturating_sub(1).min(dw - 1), y));
                points.push((dx.min(dw - 1), y));
            }
        }
    }
    for k in [0usize, 1, 2, 30, 66] {
        for edge in [16 * k + 3, 16 * k + 13] {
            let dy = edge * dh / sh;
            for x in [0usize, 7, 777, dw - 1] {
                points.push((x, dy.saturating_sub(1).min(dh - 1)));
                points.push((x, dy.min(dh - 1)));
            }
        }
    }
    compare_with_reference("text-like 1920x1080", &src, sw, sh, dw, dh, &points);
    let src = noise(sw, sh, 5);
    let points: Vec<(usize, usize)> = (0..60).map(|i| ((i * 977) % dw, (i * 389) % dh)).collect();
    compare_with_reference("noise 1920x1080", &src, sw, sh, dw, dh, &points);
}

#[test]
fn one_colour_stays_exactly_that_colour_and_the_buffers_do_not_depend_on_the_height() {
    let colour: Vec<u8> = (0..1920 * 1080).flat_map(|_| [17u8, 200, 255]).collect();
    let out = shrink_all(&colour, 1920, 1080, 1568, 882);
    assert!(out.chunks(3).all(|p| p == [17, 200, 255]), "a flat picture must stay flat");
    let odd: Vec<u8> = (0..7 * 5).flat_map(|_| [255u8, 0, 1]).collect();
    assert!(shrink_all(&odd, 7, 5, 3, 2).chunks(3).all(|p| p == [255, 0, 1]));
    let (short, tall) = (Shrinker::new(1920, 100, 1568, 80), Shrinker::new(1920, 100_000, 1568, 80_000));
    assert_eq!(short.buffer_bytes(), tall.buffer_bytes());
    eprintln!("capture: shrinker buffers at 1920 -> 1568: {} bytes", short.buffer_bytes());
}

// ---- the arguments ---------------------------------------------------------------------------------------------------------

fn args(words: &[&str]) -> Result<Request, CapError> {
    parse_args(&words.iter().map(|w| w.to_string()).collect::<Vec<_>>())
}

#[test]
fn the_arguments_are_parsed_and_every_bad_combination_is_a_usage_error() {
    let r = args(&["--out", "/CANARY-x/a.png"]).unwrap();
    assert_eq!((r.kind, r.out), (Kind::Screen, PathBuf::from("/CANARY-x/a.png")), "screen is the default");
    assert_eq!(args(&["--screen", "--out", "/CANARY-x/a.png"]).unwrap().kind, Kind::Screen);
    assert_eq!(args(&["--window", "1234", "--out", "/CANARY-x/a.png"]).unwrap().kind, Kind::Window(1234));
    assert_eq!(args(&["--out", "/CANARY-x/a.png", "--window", "0x1A00007"]).unwrap().kind, Kind::Window(0x1A00007));
    assert_eq!(args(&["--region", "1,2,30,40", "--out", "/CANARY-x/a.png"]).unwrap().kind, Kind::Region { x: 1, y: 2, w: 30, h: 40 });
    assert!(args(&["--out", "rel.png"]).unwrap().out.is_absolute(), "the path in the answer is absolute");
    let usage: [&[&str]; 13] = [
        &[],
        &["--screen"],
        &["--out"],
        &["--out", "/a.png", "--out", "/b.png"],
        &["--screen", "--window", "5", "--out", "/a.png"],
        &["--window", "5", "--region", "1,1,1,1", "--out", "/a.png"],
        &["--region", "1,1,1,1", "--screen", "--out", "/a.png"],
        &["--window", "--out", "/a.png"],
        &["--window", "abc", "--out", "/a.png"],
        &["--window", "0x", "--out", "/a.png"],
        &["--window", "99999999999", "--out", "/a.png"],
        &["--window", "-3", "--out", "/a.png"],
        &["--bogus", "--out", "/a.png"],
    ];
    for words in usage {
        assert_eq!(args(words).map(|_| ()).unwrap_err().reason, "usage", "{words:?}");
    }
    for bad in ["1,2,3", "1,2,3,4,5", "a,b,c,d", "1,2,3.5,4", "", "1,2,3,"] {
        assert_eq!(args(&["--region", bad, "--out", "/a.png"]).map(|_| ()).unwrap_err().reason, "bad_region", "{bad:?}");
    }
}

#[test]
fn an_error_answers_with_one_machine_readable_reason() {
    let e = CapError::new("bad_region", "x");
    assert_eq!(e.to_json(), json!({"ok": 0, "error": "bad_region", "detail": "x"}));
}

#[test]
fn x_errors_are_named_and_a_window_error_is_no_such_window() {
    use x11rb::x11_utils::X11Error;
    let err = |kind: ErrorKind| ReplyError::X11Error(X11Error { error_kind: kind, error_code: 3, sequence: 1, bad_value: 7, minor_opcode: 0, major_opcode: 73, extension_name: None, request_name: None });
    let e = x_error(err(ErrorKind::Match), false);
    assert_eq!((e.reason, e.detail.as_str()), ("x_error", "BadMatch"));
    let e = x_error(err(ErrorKind::Match), true);
    assert_eq!((e.reason, e.detail.as_str()), ("x_error", "BadMatch"), "a Match is not a missing window");
    let e = x_error(err(ErrorKind::Window), true);
    assert_eq!((e.reason, e.detail.as_str()), ("no_such_window", "BadWindow"));
    let e = x_error(err(ErrorKind::Drawable), true);
    assert_eq!(e.reason, "no_such_window");
    let e = x_error(err(ErrorKind::Window), false);
    assert_eq!(e.reason, "x_error", "a bad window in a read of the screen is the server's error");
    let e = x_error(x11rb::errors::ConnectionError::UnknownError, false);
    assert_eq!(e.reason, "x_error");
}

#[test]
fn the_clip_keeps_what_is_on_the_screen() {
    assert_eq!(clip((10, 20, 30, 40), 100, 100), Some((10, 20, 30, 40)));
    assert_eq!(clip((-5, -6, 30, 40), 100, 100), Some((0, 0, 25, 34)));
    assert_eq!(clip((90, 95, 30, 40), 100, 100), Some((90, 95, 10, 5)));
    assert_eq!(clip((100, 0, 5, 5), 100, 100), None);
    assert_eq!(clip((-10, 0, 10, 5), 100, 100), None);
    assert_eq!(clip((-10, -10, 200, 200), 100, 100), Some((0, 0, 100, 100)));
}

// ---- the output files ------------------------------------------------------------------------------------------------------

#[test]
fn a_pending_file_is_private_never_follows_a_link_and_leaves_nothing_behind() {
    use std::io::Write;
    let dir = Scratch::new("capout");
    let dest = dir.0.join("shot.png");
    // dropped without commit: nothing is left
    let (p, mut f) = out::Pending::create(&dest).unwrap();
    f.write_all(b"half").unwrap();
    assert_eq!(mode_of(&p.tmp), 0o600);
    drop(f);
    drop(p);
    assert!(std::fs::read_dir(&dir.0).unwrap().next().is_none(), "nothing may be left after an abandoned file");
    // committed: the final name has the bytes and the mode
    let (p, mut f) = out::Pending::create(&dest).unwrap();
    f.write_all(b"whole").unwrap();
    drop(f);
    assert_eq!(p.len().unwrap(), 5);
    p.commit().unwrap();
    assert_eq!((std::fs::read(&dest).unwrap(), mode_of(&dest)), (b"whole".to_vec(), 0o600));
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
    // a link at the final name is replaced, the file it points to is not written
    let victim = dir.0.join("victim.txt");
    std::fs::write(&victim, b"keep").unwrap();
    let linked = dir.0.join("linked.png");
    std::os::unix::fs::symlink(&victim, &linked).unwrap();
    let (p, mut f) = out::Pending::create(&linked).unwrap();
    f.write_all(b"new").unwrap();
    drop(f);
    p.commit().unwrap();
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep");
    assert!(!std::fs::symlink_metadata(&linked).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read(&linked).unwrap(), b"new");
    // a folder that does not exist is an error and creates nothing
    assert!(out::Pending::create(&dir.0.join("nope/x.png")).is_err());
    assert!(!dir.0.join("nope").exists());
}

#[test]
fn the_agent_file_is_named_after_the_output() {
    assert_eq!(agent_path_of(Path::new("/CANARY-x/shot.png")), PathBuf::from("/CANARY-x/shot.agent.png"));
    assert_eq!(agent_path_of(Path::new("/CANARY-x/shot")), PathBuf::from("/CANARY-x/shot.agent.png"));
}
