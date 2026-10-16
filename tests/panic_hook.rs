//! The panic hook is process-global, so this test gets its own binary to keep
//! panics from other tests out of its counter.

#![cfg(feature = "panic-hook")]

use std::{
    panic,
    sync::{
        Arc,
        atomic::{
            AtomicU8,
            Ordering,
        },
    },
};

#[test]
fn panic_hook_replaces_the_previous_hook() {
    let previous_calls = Arc::new(AtomicU8::new(0));
    let counter = Arc::clone(&previous_calls);
    panic::set_hook(Box::new(move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
    }));

    misstep::install_panic_hook();

    assert!(panic::catch_unwind(|| panic!("string payload")).is_err());
    assert!(panic::catch_unwind(|| panic::panic_any(67_u8)).is_err());
    assert_eq!(previous_calls.load(Ordering::SeqCst), 0);
}
