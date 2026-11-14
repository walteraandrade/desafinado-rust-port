pub const IDLE_TIMEOUT_MS: u64 = 2000;
pub const FALLBACK_DELAY_MS: u64 = 1;

pub trait IdleTarget {
    fn request_idle_callback(&mut self, run: Box<dyn FnOnce()>, timeout_ms: u64) -> Option<u64>;
    fn cancel_idle_callback(&mut self, id: u64);
    fn set_timeout(&mut self, run: Box<dyn FnOnce()>, delay_ms: u64) -> u64;
    fn clear_timeout(&mut self, id: u64);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleHandle {
    Idle(u64),
    Timer(u64),
}

pub fn when_idle(target: &mut impl IdleTarget, run: impl FnOnce() + 'static) -> IdleHandle {
    let run: Box<dyn FnOnce()> = Box::new(run);
    let shared = std::rc::Rc::new(std::cell::RefCell::new(Some(run)));
    let for_idle = std::rc::Rc::clone(&shared);
    let idle = target.request_idle_callback(
        Box::new(move || {
            if let Some(run) = for_idle.borrow_mut().take() {
                run();
            }
        }),
        IDLE_TIMEOUT_MS,
    );
    match idle {
        Some(id) => IdleHandle::Idle(id),
        None => IdleHandle::Timer(target.set_timeout(
            Box::new(move || {
                if let Some(run) = shared.borrow_mut().take() {
                    run();
                }
            }),
            FALLBACK_DELAY_MS,
        )),
    }
}

pub fn cancel_idle(target: &mut impl IdleTarget, handle: IdleHandle) {
    match handle {
        IdleHandle::Idle(id) => target.cancel_idle_callback(id),
        IdleHandle::Timer(id) => target.clear_timeout(id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    const IDLE_ID: u64 = 7;
    const TIMER_ID: u64 = 9;

    struct Scheduler {
        supports_idle: bool,
        idle: Vec<Box<dyn FnOnce()>>,
        timers: Vec<Box<dyn FnOnce()>>,
        idle_timeouts: Vec<u64>,
        cancelled_idle: Vec<u64>,
        cleared_timers: Vec<u64>,
        timer_calls: usize,
    }

    impl Scheduler {
        fn new(supports_idle: bool) -> Self {
            Self {
                supports_idle,
                idle: Vec::new(),
                timers: Vec::new(),
                idle_timeouts: Vec::new(),
                cancelled_idle: Vec::new(),
                cleared_timers: Vec::new(),
                timer_calls: 0,
            }
        }
    }

    impl IdleTarget for Scheduler {
        fn request_idle_callback(
            &mut self,
            run: Box<dyn FnOnce()>,
            timeout_ms: u64,
        ) -> Option<u64> {
            if !self.supports_idle {
                return None;
            }
            self.idle.push(run);
            self.idle_timeouts.push(timeout_ms);
            Some(IDLE_ID)
        }

        fn cancel_idle_callback(&mut self, id: u64) {
            self.cancelled_idle.push(id);
        }

        fn set_timeout(&mut self, run: Box<dyn FnOnce()>, _: u64) -> u64 {
            self.timers.push(run);
            self.timer_calls += 1;
            TIMER_ID
        }

        fn clear_timeout(&mut self, id: u64) {
            self.cleared_timers.push(id);
        }
    }

    #[test]
    fn waits_for_an_idle_period_with_a_timeout_so_a_busy_page_still_runs_it() {
        let mut target = Scheduler::new(true);
        let ran = Rc::new(Cell::new(0));
        let counter = Rc::clone(&ran);
        when_idle(&mut target, move || counter.set(counter.get() + 1));
        assert_eq!(ran.get(), 0);
        target.idle.remove(0)();
        assert_eq!(ran.get(), 1);
        assert_eq!(target.idle_timeouts, [2000]);
        assert_eq!(target.timer_calls, 0);
    }

    #[test]
    fn cancels_the_idle_callback() {
        let mut target = Scheduler::new(true);
        let handle = when_idle(&mut target, || {});
        cancel_idle(&mut target, handle);
        assert_eq!(target.cancelled_idle, [IDLE_ID]);
    }

    #[test]
    fn falls_back_to_a_timer_where_request_idle_callback_is_missing() {
        let mut target = Scheduler::new(false);
        let ran = Rc::new(RefCell::new(0));
        let counter = Rc::clone(&ran);
        let handle = when_idle(&mut target, move || *counter.borrow_mut() += 1);
        target.timers.remove(0)();
        cancel_idle(&mut target, handle);
        assert_eq!(*ran.borrow(), 1);
        assert_eq!(target.cleared_timers, [TIMER_ID]);
    }
}
