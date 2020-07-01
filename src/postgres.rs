use std::ops::Deref;

use diesel::backend::{Backend, UsesAnsiSavepointSyntax};
use diesel::connection::{AnsiTransactionManager, SimpleConnection};
use diesel::debug_query;
use diesel::deserialize::QueryableByName;
use diesel::prelude::*;
use diesel::query_builder::{AsQuery, QueryFragment, QueryId};
use diesel::sql_types::HasSqlType;

use super::{log_query, DbLogMode, LoggingConnection};

impl<C: Connection> LoggingConnection<C> {
    pub fn new(conn: C, log_mode: DbLogMode) -> Self {
        LoggingConnection { conn, log_mode }
    }

    /// This is important becase it might be needed.
    pub fn set_log_mode(&mut self, log_mode: DbLogMode) {
        self.log_mode = log_mode;
    }
}

impl<C: Connection> Deref for LoggingConnection<C> {
    type Target = C;
    fn deref(&self) -> &Self::Target {
        &self.conn
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
    C: Connection<TransactionManager = AnsiTransactionManager> + Send + 'static,
    C::Backend: UsesAnsiSavepointSyntax,
    <C::Backend as Backend>::QueryBuilder: Default,
{
    type Backend = C::Backend;
    type TransactionManager = C::TransactionManager;

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

    fn query_by_index<T, U>(&self, source: T) -> QueryResult<Vec<U>>
    where
        T: AsQuery,
        T::Query: QueryFragment<Self::Backend> + QueryId,
        Self::Backend: HasSqlType<T::SqlType>,
        U: Queryable<T::SqlType, Self::Backend>,
    {
        let query = source.as_query();

        if self.log_mode.do_not_log() {
            self.conn.query_by_index(query)
        } else {
            let debug_query = debug_query::<Self::Backend, _>(&query).to_string();

            let time_utc = chrono::Utc::now();
            let start_time = std::time::Instant::now();

            let result = self.conn.query_by_index(query);
            let duration = start_time.elapsed();

            log_query(&debug_query, duration, time_utc, self.log_mode);
            result
        }
    }

    fn query_by_name<T, U>(&self, source: &T) -> QueryResult<Vec<U>>
    where
        T: QueryFragment<Self::Backend> + QueryId,
        U: QueryableByName<Self::Backend>,
    {
        if self.log_mode.do_not_log() {
            self.conn.query_by_name(source)
        } else {
            let debug_query = debug_query::<Self::Backend, _>(&source).to_string();

            let time_utc = chrono::Utc::now();
            let start_time = std::time::Instant::now();
            let result = self.conn.query_by_name(source);
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
            let debug_query = debug_query::<Self::Backend, _>(&source).to_string();

            let time_utc = chrono::Utc::now();
            let start_time = std::time::Instant::now();

            let result = self.conn.execute_returning_count(source);
            let duration = start_time.elapsed();

            log_query(&debug_query, duration, time_utc, self.log_mode);
            result
        }
    }

    fn transaction_manager(&self) -> &Self::TransactionManager {
        self.conn.transaction_manager()
    }
}

// This section contains the implementations that allow `LoggingConnection` to work with the GST
// `PoolBuilder`.
// TODO: Implement the traits to allow us to use `OciConnection` with `LoggingConnection`.
use diesel::associations::HasTable;
use diesel::dsl::Update;
use diesel::query_builder::AsChangeset;
use diesel::query_builder::IntoUpdateTarget;
use diesel::query_dsl::{LoadQuery, UpdateAndFetchResults};

#[cfg(feature = "postgres")]
impl diesel::r2d2::R2D2Connection for LoggingConnection<::diesel::PgConnection> {
    fn ping(&self) -> QueryResult<()> {
        self.execute("SELECT 1").map(|_| ())
    }
}

#[cfg(feature = "postgres")]
impl diesel::migration::MigrationConnection for LoggingConnection<::diesel::PgConnection> {
    fn setup(&self) -> QueryResult<usize> {
        diesel::sql_query(diesel::migration::CREATE_MIGRATIONS_TABLE).execute(self)
    }
}

#[cfg(feature = "postgres")]
impl<Changes, Output> UpdateAndFetchResults<Changes, Output>
    for LoggingConnection<::diesel::PgConnection>
where
    Changes: Copy + AsChangeset<Target = <Changes as HasTable>::Table> + IntoUpdateTarget,
    Update<Changes, Changes>: LoadQuery<PgConnection, Output>,
{
    fn update_and_fetch(&self, changeset: Changes) -> QueryResult<Output> {
        diesel::dsl::update(changeset)
            .set(changeset)
            .get_result(self)
    }
}
