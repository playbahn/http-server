#[macro_export]
macro_rules! logged_panic {
    ($buf:expr, $($arg:tt)+) => {{
        logfatal!(&$buf, $($arg)+);
        panic!($($arg)+);
    }};
}

#[macro_export]
macro_rules! log {
    ($buf:expr, $level:expr, $($arg:tt)+) => {{
        $crate::log::log(
            &$buf,
            format!("{} {} {}\n",
                chrono::Local::now().format("%+"),
                $level,
                format_args!($($arg)+)
            )
        );
    }};
}

#[macro_export]
macro_rules! loginfo {
    ($buf:expr, $($arg:tt)+) => {{
        log!(&$buf, $crate::log::LogLevel::Info, $($arg)+);
    }};
}

#[macro_export]
macro_rules! logwarn {
    ($buf:expr, $($arg:tt)+) => {{
        log!(&$buf, $crate::log::LogLevel::Warn, $($arg)+);
    }};
}

#[macro_export]
macro_rules! logerr {
    ($buf:expr, $($arg:tt)+) => {{
        log!(&$buf, $crate::log::LogLevel::Error, $($arg)+);
    }};
}

#[macro_export]
macro_rules! logfatal {
    ($buf:expr, $($arg:tt)+) => {{
        log!(&$buf, $crate::log::LogLevel::Fatal, $($arg)+);
    }};
}

#[macro_export]
macro_rules! logtrace {
    ($buf:expr, $($arg:tt)+) => {{
        log!(&$buf, $crate::log::LogLevel::Trace, $($arg)+);
    }};
}

#[macro_export]
macro_rules! logdbg {
    ($buf:expr, $($arg:tt)+) => {{
        log!(&$buf, $crate::log::LogLevel::Debug, $($arg)+);
    }};
}
