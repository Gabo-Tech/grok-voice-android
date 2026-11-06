use std::sync::atomic::{AtomicBool, Ordering};

static PUBLIC_SEARCH: AtomicBool = AtomicBool::new(false);

pub fn set_public_search(enabled: bool) {
    PUBLIC_SEARCH.store(enabled, Ordering::Relaxed);
}

pub fn public_search_enabled() -> bool {
    PUBLIC_SEARCH.load(Ordering::Relaxed)
}
