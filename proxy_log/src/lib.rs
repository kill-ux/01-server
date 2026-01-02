use std::time::{SystemTime, UNIX_EPOCH};

pub fn format_time(now: SystemTime) -> String {
    let duration = now.duration_since(UNIX_EPOCH).unwrap();
    let secs = duration.as_secs();
    
    let year = 1970 + (secs / 31_557_600);  // Rough years
    let month = ((secs % 31_557_600) / 2_628_000) as u8 + 1;
    let day = ((secs % 2_628_000) / 86_400) as u8 + 1;
    let hour = ((secs % 86_400) / 3600) as u8;
    let minute = ((secs % 3600) / 60) as u8;
    let second = (secs % 60) as u8;
    
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hour, minute, second)
}

#[macro_export]
macro_rules! log {
    ($level:expr, $color:expr, $($arg:tt)*) => {
        let ts = $crate::format_time(std::time::SystemTime::now());
        println!(
            "[{}] \x1b[30m #|| web-server ||# \x1b[0 \x1b[{}m{}\x1b[0m: {}",
            ts,
            $color,
            $level,
            format!($($arg)*)
        );
    };
}


#[macro_export]
macro_rules! info { ($($arg:tt)*) => { $crate::log!("INFO ", "32", $($arg)*); }; } // Green
#[macro_export]
macro_rules! warn { ($($arg:tt)*) => { $crate::log!("WARN ", "33", $($arg)*); }; } // Yellow
#[macro_export]
macro_rules! errors { ($($arg:tt)*) => { $crate::log!("ERROR", "31", $($arg)*); }; } // Red
#[macro_export]
macro_rules! debug { ($($arg:tt)*) => { $crate::log!("DEBUG", "36", $($arg)*); }; } // Cyan
#[macro_export]
macro_rules! trace { ($($arg:tt)*) => { $crate::log!("TRACE", "34", $($arg)*); }; } // Blue