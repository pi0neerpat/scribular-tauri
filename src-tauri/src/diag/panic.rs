use std::sync::Once;
use std::{cell::Cell, panic::AssertUnwindSafe};

static INSTALL: Once = Once::new();
thread_local! { static HANDLED: Cell<bool> = const { Cell::new(false) }; }

pub fn install() {
    INSTALL.call_once(|| {
        std::panic::set_hook(Box::new(|_| {
            if !HANDLED.with(Cell::get) {
                super::log::log("PANIC", "panic", "application panic");
            }
        }))
    });
}

pub fn catch_unwind<R>(f: impl FnOnce() -> R) -> std::thread::Result<R> {
    HANDLED.with(|handled| {
        let previous = handled.replace(true);
        let result = std::panic::catch_unwind(AssertUnwindSafe(f));
        handled.set(previous);
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_only_panics_inside_recovery_regions_as_handled() {
        assert!(!HANDLED.with(Cell::get));
        let handled_inside = Cell::new(false);
        let result = catch_unwind(|| {
            handled_inside.set(HANDLED.with(Cell::get));
            panic!("recovered");
        });
        assert!(result.is_err());
        assert!(handled_inside.get());
        assert!(!HANDLED.with(Cell::get));
    }
}
