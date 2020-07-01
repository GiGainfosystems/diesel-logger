//! Currently this carte, with the given dependencies only supports postgres.
//! Given a small patch to `diesel_oci` it can likewise support oracle. However as of now,
//! The oracle mod will not compile.
#[cfg(feature = "oracle")]
pub mod oci;
#[cfg(feature = "postgres")]
pub mod postgres;

extern crate chrono;
extern crate diesel;
#[macro_use]
extern crate log;
#[cfg(feature = "oracle")]
extern crate diesel_oci;

use diesel::prelude::*;
use std::time::Duration;

/// A log mode which determines the type of logging connection is established.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DbLogMode {
    /// Do not log.
    NoLog,
    /// Log in moderation.
    Standard,
    /// Log everything if server is run in verbose mode.
    Verbose,
    /// Log everything all the time.
    Excessive,
    /// Log everything all the time, but shorten the records so we print only the start of a query.
    ExcessiveMini,
}

impl DbLogMode {
    pub fn from_env() -> Self {
        if let Ok(mode) = ::std::env::var("GST_DATABASE_LOGGING") {
            let mode = mode.to_lowercase();
            match mode.as_str() {
                "standard" => DbLogMode::Standard,
                "verbose" => DbLogMode::Verbose,
                "excessive" => DbLogMode::Excessive,
                "excessive-mini" => DbLogMode::ExcessiveMini,
                _ => DbLogMode::NoLog,
            }
        } else {
            DbLogMode::NoLog
        }
    }

    pub fn do_not_log(self) -> bool {
        self == DbLogMode::NoLog
    }
}

/// Wraps a diesel `Connection` to time and log each query using
/// the configured logger for the `log` crate.
///
/// Currently, this produces a `debug` log on every query,
/// an `info` on queries that take longer than 1 second,
/// and a `warn`ing on queries that take longer than 5 seconds.
/// These thresholds will be configurable in a future version.
pub struct LoggingConnection<C: Connection> {
    pub conn: C,
    pub log_mode: DbLogMode,
}

/// This function now takes a `chrono::DateTime` for logging in `ExcessiveMode`, which uses `println`
/// and can be accomplished even when general `gst-server` logging is disabled.
/// Also the `DbLogMode` determines the type of logging.
fn log_query(
    query: &str,
    duration: Duration,
    start_time: chrono::DateTime<chrono::Utc>,
    db_log_mode: DbLogMode,
) {
    // SAN check.
    debug_assert!(!db_log_mode.do_not_log());

    // Make query string.
    let query = query.chars().collect::<Vec<char>>();
    let query = if db_log_mode != DbLogMode::ExcessiveMini {
        query.iter().collect::<String>()
    } else {
        query.iter().take(40).collect::<String>()
    };

    match db_log_mode {
        DbLogMode::Standard => {
            if duration.as_secs() >= 5 {
                warn!(
                    "Slow query ran in {:.2} seconds: {}",
                    duration_to_secs(duration),
                    query
                );
            } else if duration.as_secs() >= 1 {
                info!(
                    "Slow query ran in {:.2} seconds: {}",
                    duration_to_secs(duration),
                    query
                );
            } else {
                debug!("Query ran in {:.1} ms: {}", duration_to_ms(duration), query);
            }
        }
        DbLogMode::Verbose => {
            if duration.as_secs() >= 1 {
                warn!(
                    "Slow query ran in {:.2} seconds: {}",
                    duration_to_secs(duration),
                    query
                );
            } else {
                warn!("Query ran in {:.1} ms: {}", duration_to_ms(duration), query);
            }
        }
        DbLogMode::Excessive | DbLogMode::ExcessiveMini => {
            if duration.as_secs() >= 1 {
                println!(
                    "[{}]: Slow query ran in {:.2} seconds: {}",
                    start_time,
                    duration_to_secs(duration),
                    query
                );
            } else {
                println!(
                    "[{}]: Query ran in {:.1} ms: {}",
                    start_time,
                    duration_to_ms(duration),
                    query
                );
            }
        }
        DbLogMode::NoLog => unreachable!("NoLog mode active. Should not be loggin."),
    }
}

const NANOS_PER_MILLI: u32 = 1_000_000;
const MILLIS_PER_SEC: u32 = 1_000;

fn duration_to_secs(duration: Duration) -> f32 {
    duration_to_ms(duration) / MILLIS_PER_SEC as f32
}

fn duration_to_ms(duration: Duration) -> f32 {
    (duration.as_secs() as u32 * 1000) as f32
        + (duration.subsec_nanos() as f32 / NANOS_PER_MILLI as f32)
}
