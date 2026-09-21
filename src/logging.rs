use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use log::{LevelFilter, Log, Metadata, Record};

const LOG_FILE: &str = "atray.log";
const LEVEL_ENV: &str = "RUST_LOG";

struct Logger {
    file: Mutex<Option<File>>,
}

static LOGGER: Logger = Logger {
    file: Mutex::new(None),
};

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let line = format!(
            "{} {:<5} {}: {}",
            timestamp(),
            record.level(),
            record.target(),
            record.args()
        );

        let mut file = self.file.lock().unwrap_or_else(|error| error.into_inner());

        if let Some(file) = file.as_mut() {
            let _ = writeln!(file, "{line}");
        }

        let _ = writeln!(io::stderr(), "{line}");
    }

    fn flush(&self) {
        let mut file = self.file.lock().unwrap_or_else(|error| error.into_inner());

        if let Some(file) = file.as_mut() {
            let _ = file.flush();
        }

        let _ = io::stderr().flush();
    }
}

pub fn init(directory: &Path) {
    let level = level();
    let path = directory.join(LOG_FILE);

    let (file, error) = match OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
    {
        Ok(file) => (Some(file), None),
        Err(error) => (None, Some(error)),
    };

    *LOGGER
        .file
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = file;

    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(level);
    }

    match error {
        Some(error) => log::warn!("failed to open log file {}: {error}", path.display()),
        None => log::info!("log file: {}", path.display()),
    }
}

pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let backtrace = std::backtrace::Backtrace::force_capture();

        log::error!("panic: {info}\n{backtrace}");
        log::logger().flush();
    }));
}

fn level() -> LevelFilter {
    std::env::var(LEVEL_ENV)
        .ok()
        .and_then(|value| parse_level(value.rsplit('=').next().unwrap_or(&value)))
        .unwrap_or(LevelFilter::Info)
}

fn parse_level(level: &str) -> Option<LevelFilter> {
    match level.trim().to_ascii_lowercase().as_str() {
        "off" => Some(LevelFilter::Off),
        "error" => Some(LevelFilter::Error),
        "warn" => Some(LevelFilter::Warn),
        "info" => Some(LevelFilter::Info),
        "debug" => Some(LevelFilter::Debug),
        "trace" => Some(LevelFilter::Trace),
        _ => None,
    }
}

fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let seconds = now.as_secs();
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400) as i64);

    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        (seconds / 3_600) % 24,
        (seconds / 60) % 60,
        seconds % 60,
        now.subsec_millis()
    )
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_part + 2) / 5 + 1) as u32;
    let month = if month_part < 10 {
        month_part + 3
    } else {
        month_part - 9
    } as u32;
    let year = year_of_era as i64 + era * 400 + if month <= 2 { 1 } else { 0 };

    (year, month, day)
}
