//! The breathing marks of the requests the tray shows as working, drawn by a
//! view of their own over the editor pane, as the AI caret is. A mark breathes
//! for as long as its request runs; drawn inside the pane, each of its frames
//! would render the pane again too. The tray leaves an empty spot for each
//! mark and records where it is laid out (`EditorPane::tray_mark_spots`).

use ginga_gpui::Island;
use gpui_kit::*;

use crate::{editor_pane::EditorPane, theme, ui};

pub struct TrayMarks {
    pane: WeakEntity<EditorPane>,
}

pub type TrayMarksView = Island<TrayMarks>;

pub fn new(pane: &Entity<EditorPane>, cx: &mut App) -> Entity<TrayMarksView> {
    let marks = TrayMarks {
        pane: pane.downgrade(),
    };
    cx.new(|_| Island::new(marks, draw))
}

fn draw(marks: &mut TrayMarks, _: &mut Window, cx: &mut Context<TrayMarksView>) -> AnyElement {
    let Some(pane) = marks.pane.upgrade() else {
        return Empty.into_any_element();
    };
    let pane = pane.read(cx);
    let spots = pane.tray_mark_spots();
    if spots.is_empty() {
        return Empty.into_any_element();
    }
    let area = pane.pane_bounds().size;
    let roles = theme::roles(cx);
    div()
        .absolute()
        .top_0()
        .left_0()
        .w(area.width)
        .h(area.height)
        .overflow_hidden()
        .children(spots.into_iter().map(|(id, spot)| {
            div()
                .absolute()
                .left(spot.x)
                .top(spot.y)
                .child(ui::job_mark(
                    SharedString::from(format!("job-mark-{id}")),
                    true,
                    roles,
                ))
        }))
        .into_any_element()
}
