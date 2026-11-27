//! The AI caret, drawn by a view of its own beside the editor pane. Its
//! breathing and its trips ask for frames of this view alone, so the pane,
//! which the workspace caches, is not rendered again for them.
//!
//! So the caret is painted above the pane's Cards, bubble and requests tray
//! where they overlap. Drawn inside the pane, under them, every frame it asks
//! for would render the pane again too: GPUI renders again the view that asks
//! for a frame and every view above it.

use desafinado_ai::editor::caret_travel::{CaretTravel, Spot};
use ginga_gpui::Island;
use gpui_kit::{prelude::FluentBuilder as _, *};

use crate::{
    editor_pane::{AiCaretAt, AiCaretPlace, EditorPane},
    theme, ui,
};

pub struct AiCaret {
    pane: WeakEntity<EditorPane>,
    travel: CaretTravel,
    clock: ui::MotionClock,
}

pub type AiCaretView = Island<AiCaret>;

pub fn new(pane: &Entity<EditorPane>, cx: &mut App) -> Entity<AiCaretView> {
    let caret = AiCaret {
        pane: pane.downgrade(),
        travel: CaretTravel::default(),
        clock: ui::MotionClock::default(),
    };
    cx.new(|_| Island::new(caret, draw))
}

fn draw(caret: &mut AiCaret, window: &mut Window, cx: &mut Context<AiCaretView>) -> AnyElement {
    let place = caret
        .pane
        .upgrade()
        .map_or(AiCaretPlace::Gone, |pane| pane.read(cx).ai_caret_place(cx));
    let at = match place {
        AiCaretPlace::Gone => {
            caret.travel.land();
            return Empty.into_any_element();
        }
        AiCaretPlace::Hidden => {
            caret.travel.hide();
            return Empty.into_any_element();
        }
        AiCaretPlace::At(at) => at,
    };
    let (trip, now) = ui::travel(&mut caret.clock, cx);
    let frame = caret.travel.step(at.pos, at.spot, at.takeoff, trip, now);
    if frame.moving {
        window.request_animation_frame();
    }
    caret_over_pane(&at, frame.spot, cx)
}

/// The caret at `spot`, on a layer the size of the pane that clips it.
fn caret_over_pane(at: &AiCaretAt, spot: Spot, cx: &App) -> AnyElement {
    let roles = theme::roles(cx);
    let flag = ui::ai_caret_flag(at.label.clone(), roles, cx.reduce_motion())
        .absolute()
        .top(px(-16.))
        .when(at.flipped, |this| this.right(px(0.)))
        .when(!at.flipped, |this| this.left(px(0.)));
    let bar = ui::breathe(
        "ai-caret-bar",
        div().absolute().top_0().left_0().size_full().bg(roles.ai),
    );
    div()
        .absolute()
        .top_0()
        .left_0()
        .w(at.area.width)
        .h(at.area.height)
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .left(px(spot.0))
                .top(px(spot.1))
                .w(px(2.))
                .h(at.height)
                .child(bar)
                .child(ui::enter(
                    "ai-flag",
                    flag,
                    ui::Enter::Rise(3.),
                    ui::MOTION_ENTER,
                )),
        )
        .into_any_element()
}
