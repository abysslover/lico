//! Helpers shared by the contract tests: a terminal, text in, the rows as text out.
#![allow(dead_code)]

use lico_vt::{Attrs, Cell, Term};

pub fn term(cols: u16, rows: u16) -> Term {
    Term::new(cols, rows)
}

pub fn feed(t: &mut Term, s: &str) {
    t.feed(s.as_bytes());
}

/// A line as text: tails add nothing, the trailing blanks are cut.
pub fn line_text(l: &[Cell]) -> String {
    let s: String = l.iter().filter(|c| !c.attrs.contains(Attrs::TAIL)).map(|c| c.ch).collect();
    s.trim_end().to_string()
}

/// The rows of the screen as text.
pub fn rows(t: &Term) -> Vec<String> {
    t.view(0).lines.iter().map(|l| line_text(l)).collect()
}

pub fn row(t: &Term, r: usize) -> String {
    rows(t)[r].clone()
}

/// The cell at (row, col) of the screen.
pub fn at(t: &Term, r: u16, c: u16) -> Cell {
    t.view(0).lines[r as usize][c as usize]
}

/// `feed` of a text on a fresh `cols` x `rows` terminal.
pub fn run(cols: u16, rows_: u16, s: &str) -> Term {
    let mut t = term(cols, rows_);
    feed(&mut t, s);
    t
}

/// A tiny deterministic generator for the tests that want noise.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}
