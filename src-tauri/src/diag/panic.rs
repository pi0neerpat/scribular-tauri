use std::sync::Once;

static INSTALL: Once = Once::new();

pub fn install() {
    INSTALL.call_once(|| {
        std::panic::set_hook(Box::new(|_| {
            super::log::log("PANIC", "panic", "application panic");
        }))
    });
}
