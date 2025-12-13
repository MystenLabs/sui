// User logging wrappers for outputting structured messages to the user.
macro_rules! user_warning {
    ($($arg:tt)*) => {{
        use colored::Colorize;
        eprintln!("[{}] {}", "WARNING".bold().yellow(), format!($($arg)*));
    }};
    }

macro_rules! user_note {
    ($($arg:tt)*) => {{
        use colored::Colorize;
        eprintln!("[{}] {}", "NOTE".bold().yellow(), format!($($arg)*));
    }};
}

macro_rules! user_info {
    ($($arg:tt)*) => {{
        eprintln!("{}", format!($($arg)*));
    }};
}

macro_rules! user_error {
    ($($arg:tt)*) => {{
        use colored::Colorize;
        eprintln!("[{}] {}", "ERROR".bold().red(), format!($($arg)*));
    }};
}

pub(crate) use user_error;
pub(crate) use user_info;
pub(crate) use user_note;
pub(crate) use user_warning;
