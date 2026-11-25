//! The pixels of the decorations with a gutter and with the band of the agent's own text (`M24-P11b`).
use super::*;

const W: usize = 300;
const H: usize = 60;
const LIST_W: usize = 100;
const MARK: u32 = 0xFF0A_0B0C;

type Rows = Vec<(f32, f32, usize, u16)>;

fn rows(roles: &[u16]) -> Rows {
    roles.iter().enumerate().map(|(i, &role)| (i as f32 * 10.0, i as f32 * 10.0 + 10.0, i, role)).collect()
}

fn paint(roles: &[u16], look: Look, cleared: Option<&[(i32, i32)]>) -> Vec<u32> {
    let mut buf = vec![MARK; W * H];
    paint_transcript_decorations(&mut buf, W, H, LIST_W, 1.0, 0, H as i32, &rows(roles), cleared, look);
    buf
}

fn px(buf: &[u32], x: usize, row: usize) -> u32 {
    buf[(row * 10 + 5) * W + x]
}

const CARD: [u16; 3] = [STYLE_USER | ROLE_PEER_BG | ROLE_BOX_TOP, STYLE_USER | ROLE_PEER_BG, STYLE_USER | ROLE_PEER_BG | ROLE_BOX_BOT];
const BOX: [u16; 3] = [STYLE_CMD | ROLE_BOX | ROLE_BOX_TOP, STYLE_CMD_OUT | ROLE_BOX, STYLE_CMD_OUT | ROLE_BOX | ROLE_BOX_BOT];

#[test]
fn without_a_gutter_the_bands_run_to_the_window_edge_as_before() {
    let buf = paint(&[STYLE_USER | ROLE_USER_BG], Look::default(), None);
    assert_eq!((px(&buf, LIST_W - 1, 0), px(&buf, LIST_W, 0), px(&buf, W - 1, 0)), (MARK, USER_BG, USER_BG), "the person's band, from the list to the edge");
    let peer = paint(&CARD, Look::default(), None);
    assert_eq!((px(&peer, W - 3, 1), px(&peer, W - 2, 1)), (PEER_BORDER, MARK), "a card keeps two pixels from the edge, its border in the last of the rest");
    let boxed = paint(&BOX, Look::default(), None);
    assert_eq!((px(&boxed, W - 3, 1), px(&boxed, W - 2, 1)), (CMD_BORDER, MARK));
}

#[test]
fn with_a_gutter_every_band_and_box_ends_where_it_begins() {
    let look = Look { gutter: 60, response: true };
    let edge = W - 60;
    let user = paint(&[STYLE_USER | ROLE_USER_BG], look, None);
    assert_eq!((px(&user, edge - 1, 0), px(&user, edge, 0), px(&user, W - 1, 0)), (USER_BG, MARK, MARK), "the person's band");
    let peer = paint(&CARD, look, None);
    assert_eq!((px(&peer, edge - 3, 1), px(&peer, edge - 2, 1), px(&peer, edge, 1), px(&peer, W - 1, 1)), (PEER_BORDER, MARK, MARK, MARK), "a card: two pixels short of the gutter");
    assert_eq!((px(&peer, LIST_W + 1, 1), px(&peer, 150, 1)), (PEER_RAIL, PEER_BG), "the rail and the body keep their places");
    let boxed = paint(&BOX, look, None);
    assert_eq!((px(&boxed, edge - 3, 1), px(&boxed, edge - 2, 1), px(&boxed, W - 1, 1)), (CMD_BORDER, MARK, MARK), "a command box");
    assert_eq!(px(&boxed, 150, 1), CMD_BG);
}

#[test]
fn the_agents_own_text_has_a_band_that_ends_at_the_gutter_and_a_note_has_none() {
    let look = Look { gutter: 60, response: true };
    let edge = W - 60;
    let reply = paint(&[STYLE_AGENT, STYLE_AGENT], look, None);
    assert_eq!((px(&reply, LIST_W - 1, 0), px(&reply, LIST_W, 0), px(&reply, edge - 1, 0), px(&reply, edge, 0)), (MARK, RESPONSE_BG, RESPONSE_BG, MARK));
    assert_eq!(px(&reply, 150, 1), RESPONSE_BG, "every line of the reply");
    let full = paint(&[STYLE_AGENT], Look { gutter: 0, response: true }, None);
    assert_eq!((px(&full, LIST_W, 0), px(&full, W - 1, 0)), (RESPONSE_BG, RESPONSE_BG), "with no gutter the band runs to the edge like the person's");
    let none = paint(&[STYLE_AGENT], Look { gutter: 60, response: false }, None);
    assert_eq!(px(&none, 150, 0), MARK, "a view that is not a conversation (the page's rows) has no band");
    for note in [STYLE_ERROR, STYLE_TOOL, STYLE_USER] {
        let buf = paint(&[note], look, None);
        assert_eq!(px(&buf, 150, 0), MARK, "style {note}: no band");
    }
    assert_eq!(RESPONSE_BG, 0xFF17_1A1A);
}

#[test]
fn a_scroll_step_paints_only_the_rows_that_were_cleared() {
    let look = Look { gutter: 60, response: true };
    let buf = paint(&[STYLE_AGENT, STYLE_AGENT, STYLE_AGENT], look, Some(&[(10, 20)]));
    assert_eq!((px(&buf, 150, 0), px(&buf, 150, 1), px(&buf, 150, 2)), (MARK, RESPONSE_BG, MARK), "the other rows are what the last frame left, moved");
}

#[test]
fn a_gutter_wider_than_the_area_paints_nothing_and_does_not_panic() {
    let look = Look { gutter: 5000, response: true };
    for roles in [&[STYLE_USER | ROLE_USER_BG][..], &CARD[..], &BOX[..], &[STYLE_AGENT][..]] {
        let buf = paint(roles, look, None);
        assert!(buf.iter().all(|&p| p == MARK), "{roles:?}");
    }
}
