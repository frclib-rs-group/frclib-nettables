///upgrades a weak pointer, returns if none
macro_rules! upgrade_ret {
    ($weak:expr) => {
        match $weak.upgrade() {
            Some(value) => value,
            None => return,
        }
    };
}

///upgrades a weak pointer, breaks if none
macro_rules! upgrade_brk {
    ($weak:expr) => {
        match $weak.upgrade() {
            Some(value) => value,
            None => break,
        }
    };
}

macro_rules! lock {
    ($lock:expr) => {
        $lock.lock().await
    };
}
