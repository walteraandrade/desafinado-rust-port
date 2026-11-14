use std::rc::Rc;

#[derive(Clone)]
pub struct ReviewControls {
    pub start: Rc<dyn Fn(&str)>,
    pub stop: Rc<dyn Fn()>,
    pub accept_all: Rc<dyn Fn()>,
    pub reject_all: Rc<dyn Fn()>,
}

pub fn before_review_changes(controls: &ReviewControls, before: Rc<dyn Fn()>) -> ReviewControls {
    let start = Rc::clone(&controls.start);
    let accept_all = Rc::clone(&controls.accept_all);
    let reject_all = Rc::clone(&controls.reject_all);
    let (b1, b2, b3) = (Rc::clone(&before), Rc::clone(&before), before);
    ReviewControls {
        start: Rc::new(move |focus| {
            b1();
            start(focus);
        }),
        stop: Rc::clone(&controls.stop),
        accept_all: Rc::new(move || {
            b2();
            accept_all();
        }),
        reject_all: Rc::new(move || {
            b3();
            reject_all();
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn controls(calls: &Rc<RefCell<Vec<String>>>) -> ReviewControls {
        let push = |label: &'static str| {
            let calls = Rc::clone(calls);
            move || calls.borrow_mut().push(label.to_owned())
        };
        let start_calls = Rc::clone(calls);
        ReviewControls {
            start: Rc::new(move |focus| start_calls.borrow_mut().push(format!("start:{focus}"))),
            stop: Rc::new(push("stop")),
            accept_all: Rc::new(push("acceptAll")),
            reject_all: Rc::new(push("rejectAll")),
        }
    }

    #[test]
    fn runs_the_hook_before_a_pass_starts_and_before_accept_all_or_reject_all() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let hook_calls = Rc::clone(&calls);
        let wrapped = before_review_changes(
            &controls(&calls),
            Rc::new(move || hook_calls.borrow_mut().push("before".to_owned())),
        );
        (wrapped.start)("tone");
        (wrapped.accept_all)();
        (wrapped.reject_all)();
        assert_eq!(
            *calls.borrow(),
            [
                "before",
                "start:tone",
                "before",
                "acceptAll",
                "before",
                "rejectAll"
            ]
        );
    }

    #[test]
    fn leaves_stop_alone() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let hook_calls = Rc::clone(&calls);
        let wrapped = before_review_changes(
            &controls(&calls),
            Rc::new(move || hook_calls.borrow_mut().push("before".to_owned())),
        );
        (wrapped.stop)();
        assert_eq!(*calls.borrow(), ["stop"]);
    }
}
