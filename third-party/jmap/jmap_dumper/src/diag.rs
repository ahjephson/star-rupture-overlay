use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum Verbosity {
    Quiet = 0,
    #[default]
    Normal = 1,
    Verbose = 2,
    Trace = 3,
}

static LEVEL: AtomicU8 = AtomicU8::new(Verbosity::Normal as u8);
static WARNINGS: AtomicUsize = AtomicUsize::new(0);

pub fn set_verbosity(level: Verbosity) {
    LEVEL.store(level as u8, Ordering::Relaxed);
}

pub fn verbosity() -> Verbosity {
    match LEVEL.load(Ordering::Relaxed) {
        0 => Verbosity::Quiet,
        1 => Verbosity::Normal,
        2 => Verbosity::Verbose,
        _ => Verbosity::Trace,
    }
}

pub fn is_verbose() -> bool {
    verbosity() >= Verbosity::Verbose
}

pub fn is_trace() -> bool {
    verbosity() >= Verbosity::Trace
}

pub fn warnings_enabled() -> bool {
    verbosity() >= Verbosity::Normal
}

pub fn count_warning() {
    WARNINGS.fetch_add(1, Ordering::Relaxed);
}

pub fn warning_count() -> usize {
    WARNINGS.load(Ordering::Relaxed)
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {{
        $crate::diag::count_warning();
        if $crate::diag::warnings_enabled() {
            eprintln!("WARN: {}", format_args!($($arg)*));
        }
    }};
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        if $crate::diag::is_verbose() {
            eprintln!("{}", format_args!($($arg)*));
        }
    }};
}

#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {{
        if $crate::diag::is_trace() {
            eprintln!("{}", format_args!($($arg)*));
        }
    }};
}
