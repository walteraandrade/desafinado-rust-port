//! The AI caret, drawn by a view of its own beside the editor pane. Its
//! breathing and its trips ask for frames of this view alone, so the pane,
//! which the workspace caches, is not rendered again for them.
//!
//! So the caret is painted above everything the pane draws over its text,
//! where they overlap: the text map and its chips, the Notes' marks, an open
//! Note, the requests tray, the Suggestion Cards, the bubble and the spelling
//! menu. Drawn inside the pane, under them, every frame it asks for would
//! render the pane again too: GPUI renders again the view that asks for a
//! frame and every view above it.

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
    breath: ui::Looping<2>,
    dots: ui::Looping<2>,
}

pub type AiCaretView = Island<AiCaret>;

pub fn new(pane: &Entity<EditorPane>, cx: &mut App) -> Entity<AiCaretView> {
    let caret = AiCaret {
        pane: pane.downgrade(),
        travel: CaretTravel::default(),
        clock: ui::MotionClock::default(),
        breath: ui::breath(),
        dots: ui::dots_round(),
    };
    cx.new(|_| Island::new(caret, draw))
}

/// The editor is not on screen: the AI caret it shows next takes off again
/// from the writer's caret instead of gliding from where it was last drawn.
pub fn land(view: &Entity<AiCaretView>, cx: &mut App) {
    view.update(cx, |island, _| {
        let caret = island.state_mut();
        caret.travel.land();
        caret.away();
    });
}

fn draw(caret: &mut AiCaret, window: &mut Window, cx: &mut Context<AiCaretView>) -> AnyElement {
    let place = caret
        .pane
        .upgrade()
        .map_or(AiCaretPlace::Gone, |pane| pane.read(cx).ai_caret_place(cx));
    let at = match place {
        AiCaretPlace::Gone => {
            caret.travel.land();
            caret.away();
            return Empty.into_any_element();
        }
        AiCaretPlace::Hidden => {
            caret.travel.hide();
            caret.away();
            return Empty.into_any_element();
        }
        AiCaretPlace::At(at) => at,
    };
    let now = caret.clock.now(cx);
    let frame = caret.travel.step(
        at.target.as_deref(),
        at.spot,
        at.takeoff,
        ui::caret_trip(cx),
        now,
    );
    if frame.moving {
        window.request_animation_frame();
    }
    let breath = caret.breath.frame(window, cx);
    let dots = ui::dot_levels(caret.dots.frame(window, cx), cx.reduce_motion());
    caret_over_pane(&at, frame.spot, Pulse { breath, dots }, cx)
}

impl AiCaret {
    fn away(&mut self) {
        self.breath.away();
        self.dots.away();
    }
}

/// Where the caret's loops are this frame: the bar's opacity and the flag's dots.
struct Pulse {
    breath: f32,
    dots: [f32; 3],
}

/// The caret at `spot`, on a layer the size of the pane that clips it.
fn caret_over_pane(at: &AiCaretAt, spot: Spot, pulse: Pulse, cx: &App) -> AnyElement {
    let roles = theme::roles(cx);
    let flag = ui::ai_caret_flag(at.label.clone(), roles, pulse.dots)
        .absolute()
        .top(px(-16.))
        .when(at.flipped, |this| this.right(px(0.)))
        .when(!at.flipped, |this| this.left(px(0.)));
    let bar = ui::ai_caret_bar(roles, pulse.breath);
    ui::over_pane(at.area)
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
