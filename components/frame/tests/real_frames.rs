//! NOT part of a normal run: decodes the real frames that the engine sent (PNG: M17-P03, JPEG: M20-P02), saved under `private/scratch/`, and prints the time and a checksum of
//! the pixels for each (to be compared with an independent decode).
//!
//!     LICO_FRAME_SAMPLES=/abs/dir/with/pngs-and-jpgs cargo test --release -p lico-frame --test real_frames -- --ignored --nocapture

use lico_frame::*;
use std::time::Instant;

#[test]
#[ignore = "needs the saved engine frames: LICO_FRAME_SAMPLES (see the head of this file)"]
fn the_real_frames_of_the_engine_decode() {
    let dir = std::env::var("LICO_FRAME_SAMPLES").expect("LICO_FRAME_SAMPLES");
    let mut names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "png" || e == "jpg")).collect();
    names.sort();
    assert!(!names.is_empty(), "no .png or .jpg in {dir}");
    for p in names {
        let bytes = std::fs::read(&p).unwrap();
        let h = header(&bytes).unwrap();
        let mut best = u128::MAX;
        let mut img = None;
        for _ in 0..5 {
            let t = Instant::now();
            let i = decode(&bytes, &Limits::default()).unwrap();
            best = best.min(t.elapsed().as_micros());
            img = Some(i);
        }
        let img = img.unwrap();
        let sum = img.pixels.iter().fold(0xcbf29ce484222325u64, |h, p| (h ^ *p as u64).wrapping_mul(0x100000001b3));
        println!("FRAME {} {:?} {}x{} colour {} depth {} input {} bytes decode_best_us {} fnv {:016x} decoded_bytes {}", p.file_name().unwrap().to_string_lossy(), h.format, h.width, h.height, h.colour, h.depth, bytes.len(), best, sum, img.pixels.len() * 4);
    }
}
