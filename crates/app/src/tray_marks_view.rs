//! The breathing marks of the requests the tray shows as working, drawn by a
//! view of their own over the editor pane, as the AI caret is. A mark breathes
//! for as long as its request runs; drawn inside the pane, each of its frames
//! would render the pane again too. The tray leaves an empty spot for each
//! mark and records where it is laid out (`EditorPane::tray_mark_spots`).
//!
//! The pane is cached and lays out after this view has drawn, so the marks
//! follow their spots one frame late: when a spot appears, moves or goes, the
//! pane has this view, and only this view, drawn again on the next frame. For
//! that frame a mark can sit where its row was, or over the pass mark of a
//! request that has just answered.
//!
//! Like the AI caret, the marks are painted above everything the pane draws
//! over its text: a Suggestion Card, the bubble or the spelling menu that
//! overlaps the tray's corner, which happens in a narrow window, no longer
//! covers them.

use ginga_gpui::Island;
use gpui_kit::*;

use crate::{editor_pane::EditorPane, theme, ui};

pub struct TrayMarks {
    pane: WeakEntity<EditorPane>,
    breath: ui::Looping<2>,
}

pub type TrayMarksView = Island<TrayMarks>;

pub fn new(pane: &Entity<EditorPane>, cx: &mut App) -> Entity<TrayMarksView> {
    let marks = TrayMarks {
        pane: pane.downgrade(),
        breath: ui::breath(),
    };
    let view = cx.new(|_| Island::new(marks, draw));
    let weak = view.downgrade();
    pane.update(cx, |pane, _| pane.jobs.marks_view = Some(weak));
    view
}

/// The pane is not on screen: the marks breathe from the top when it is again.
pub fn away(view: &Entity<TrayMarksView>, cx: &mut App) {
    view.update(cx, |island, _| island.state_mut().breath.away());
}

fn draw(marks: &mut TrayMarks, window: &mut Window, cx: &mut Context<TrayMarksView>) -> AnyElement {
    let Some(pane) = marks.pane.upgrade() else {
        marks.breath.away();
        return Empty.into_any_element();
    };
    let pane = pane.read(cx);
    let spots = pane.tray_mark_spots();
    if spots.is_empty() {
        marks.breath.away();
        return Empty.into_any_element();
    }
    let area = pane.pane_bounds().size;
    let opacity = marks.breath.frame(window, cx);
    let roles = theme::roles(cx);
    ui::over_pane(area)
        .children(spots.into_iter().map(|(_, spot)| {
            div()
                .absolute()
                .left(spot.x)
                .top(spot.y)
                .child(ui::job_mark(ui::JobMark::Working { opacity }, roles))
        }))
        .into_any_element()
}
