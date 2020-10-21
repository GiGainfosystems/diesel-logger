use diesel::prelude::*;
use diesel::query_builder::{AsQuery, QueryFragment, QueryId};
use diesel::{backend::Backend, expression::QueryMetadata};
use diesel::{
    connection::{SimpleConnection, TransactionManager},
    deserialize::FromSqlRow,
};
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

impl<C: Connection> LoggingConnection<C> {
    pub fn new(conn: C, log_mode: DbLogMode) -> Self {
        LoggingConnection { conn, log_mode }
    }

    /// This is important becase it might be needed.
    pub fn set_log_mode(&mut self, log_mode: DbLogMode) {
        self.log_mode = log_mode;
    }
}

impl<C> SimpleConnection for LoggingConnection<C>
where
    C: Connection + Send + 'static,
{
    fn batch_execute(&self, query: &str) -> QueryResult<()> {
        self.conn.batch_execute(query)
    }
}

impl<C: Connection> Connection for LoggingConnection<C>
where
    C: Connection + Send + 'static,
    <C::Backend as Backend>::QueryBuilder: Default,
{
    type Backend = C::Backend;
    type TransactionManager = LoggingTransactionManager<C>;

    fn establish(database_url: &str) -> ConnectionResult<Self> {
        let log_mode = DbLogMode::from_env();
        let conn = C::establish(database_url)?;
        Ok(LoggingConnection { conn, log_mode })
    }

    fn execute(&self, query: &str) -> QueryResult<usize> {
        if self.log_mode.do_not_log() {
            self.conn.execute(query)
        } else {
            let time_utc = chrono::Utc::now();
            let start_time = std::time::Instant::now();

            let result = self.conn.execute(query);
            let duration = start_time.elapsed();

            log_query(query, duration, time_utc, self.log_mode);
            result
        }
    }

    fn load<T, U>(&self, source: T) -> QueryResult<Vec<U>>
    where
        T: AsQuery,
        T::Query: QueryFragment<Self::Backend> + QueryId,
        U: FromSqlRow<T::SqlType, Self::Backend>,
        Self::Backend: QueryMetadata<T::SqlType>,
    {
        let query = source.as_query();

        if self.log_mode.do_not_log() {
            self.conn.load(query)
        } else {
            let debug_query = diesel::debug_query::<Self::Backend, _>(&query).to_string();

            let time_utc = chrono::Utc::now();
            let start_time = std::time::Instant::now();

            let result = self.conn.load(query);
            let duration = start_time.elapsed();

            log_query(&debug_query, duration, time_utc, self.log_mode);
            result
        }
    }

    fn execute_returning_count<T>(&self, source: &T) -> QueryResult<usize>
    where
        T: QueryFragment<Self::Backend> + QueryId,
    {
        if self.log_mode.do_not_log() {
            self.conn.execute_returning_count(source)
        } else {
            let debug_query = diesel::debug_query::<Self::Backend, _>(&source).to_string();

            let time_utc = chrono::Utc::now();
            let start_time = std::time::Instant::now();

            let result = self.conn.execute_returning_count(source);
            let duration = start_time.elapsed();

            log_query(&debug_query, duration, time_utc, self.log_mode);
            result
        }
    }

    fn transaction_manager(&self) -> &Self::TransactionManager {
        // this is actually fine because we have an #[repr(transparent)]
        // on LoggingTransactionManager, which means the layout is the same
        // as the inner type
        // See the ref-cast crate for a longer version: https://github.com/dtolnay/ref-cast
        unsafe {
            &*(self.conn.transaction_manager() as *const _ as *const Self::TransactionManager)
        }
    }

    fn begin_test_transaction(&self) -> QueryResult<()> {
        self.conn.begin_test_transaction()
    }
}

#[repr(transparent)]
pub struct LoggingTransactionManager<C: Connection> {
    inner: C::TransactionManager,
}

impl<C> TransactionManager<LoggingConnection<C>> for LoggingTransactionManager<C>
where
    C: Connection + 'static,
    <C::Backend as Backend>::QueryBuilder: Default,
{
    fn begin_transaction(&self, conn: &LoggingConnection<C>) -> QueryResult<()> {
        self.inner.begin_transaction(&conn.conn)
    }

    fn rollback_transaction(&self, conn: &LoggingConnection<C>) -> QueryResult<()> {
        self.inner.rollback_transaction(&conn.conn)
    }

    fn commit_transaction(&self, conn: &LoggingConnection<C>) -> QueryResult<()> {
        self.inner.commit_transaction(&conn.conn)
    }

    fn get_transaction_depth(&self) -> u32 {
        self.inner.get_transaction_depth()
    }
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
    use std::borrow::Cow;

    // SAN check.
    debug_assert!(!db_log_mode.do_not_log());

    // Make query string.
    let query = if db_log_mode != DbLogMode::ExcessiveMini {
        Cow::Borrowed(query)
    } else {
        Cow::Owned(query.chars().take(40).collect::<String>())
    };

    match db_log_mode {
        DbLogMode::Standard => {
            if duration.as_secs() >= 5 {
                log::warn!(
                    "Slow query ran in {:.2} seconds: {}",
                    duration_to_secs(duration),
                    query
                );
            } else if duration.as_secs() >= 1 {
                log::info!(
                    "Slow query ran in {:.2} seconds: {}",
                    duration_to_secs(duration),
                    query
                );
            } else {
                log::debug!("Query ran in {:.1} ms: {}", duration_to_ms(duration), query);
            }
        }
        DbLogMode::Verbose => {
            if duration.as_secs() >= 1 {
                log::warn!(
                    "Slow query ran in {:.2} seconds: {}",
                    duration_to_secs(duration),
                    query
                );
            } else {
                log::warn!("Query ran in {:.1} ms: {}", duration_to_ms(duration), query);
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

impl<C> diesel::r2d2::R2D2Connection for LoggingConnection<C>
where
    C: diesel::r2d2::R2D2Connection,
    Self: Connection,
{
    fn ping(&self) -> QueryResult<()> {
        self.conn.ping()
    }
}

impl<C> diesel::migration::MigrationConnection for LoggingConnection<C>
where
    C: diesel::migration::MigrationConnection,
    Self: Connection,
{
    fn setup(&self) -> QueryResult<usize> {
        self.conn.setup()
    }
}

impl<Changes, Output, C> diesel::query_dsl::UpdateAndFetchResults<Changes, Output>
    for LoggingConnection<C>
where
    C: diesel::query_dsl::UpdateAndFetchResults<Changes, Output>,
    Self: Connection,
{
    fn update_and_fetch(&self, changeset: Changes) -> QueryResult<Output> {
        self.conn.update_and_fetch(changeset)
    }
}
