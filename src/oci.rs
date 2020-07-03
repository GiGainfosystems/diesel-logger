use std::ops::Deref;

use diesel::connection::TransactionManager;
use diesel::connection::{Connection, SimpleConnection};
use diesel::debug_query;
use diesel::deserialize::QueryableByName;
use diesel::migration::MigrationConnection;
use diesel::prelude::*;
use diesel::query_builder::{AsQuery, QueryFragment, QueryId};
use diesel::r2d2::R2D2Connection;
use diesel::sql_types::HasSqlType;

use super::*;
use diesel_oci::oracle::connection::transaction::OCITransactionManager;
use diesel_oci::oracle::connection::OciConnection;

// These go together.
use diesel::associations::HasTable;
use diesel::associations::Identifiable;
use diesel::dsl::Find;
use diesel::dsl::Update;
use diesel::query_builder::functions::update;
use diesel::query_builder::{AsChangeset, IntoUpdateTarget};
use diesel::query_dsl::methods::{ExecuteDsl, FindDsl};
use diesel::query_dsl::UpdateAndFetchResults;
use diesel::query_dsl::{LoadQuery, RunQueryDsl};
use diesel::result::QueryResult;

impl<Changes, Output> UpdateAndFetchResults<Changes, Output> for LoggingConnection<OciConnection>
where
    Changes: Copy + Identifiable,
    Changes: AsChangeset<Target = <Changes as HasTable>::Table> + IntoUpdateTarget,
    Changes::Table: FindDsl<Changes::Id>,
    Update<Changes, Changes>: ExecuteDsl<OciConnection>,
    Find<Changes::Table, Changes::Id>: LoadQuery<OciConnection, Output>,
{
    fn update_and_fetch(&self, changeset: Changes) -> QueryResult<Output> {
        update(changeset).set(changeset).execute(self)?;
        diesel::query_dsl::filter_dsl::FindDsl::find(Changes::table(), changeset.id())
            .get_result(&self.conn)
    }
}

impl LoggingConnection<OciConnection> {
    pub fn new(conn: OciConnection, log_mode: DbLogMode) -> Self {
        LoggingConnection { conn, log_mode }
    }

    /// This is important becase it might be needed.
    pub fn set_log_mode(&mut self, log_mode: DbLogMode) {
        self.log_mode = log_mode;
    }
}

impl Deref for LoggingConnection<OciConnection> {
    type Target = OciConnection;
    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}

impl SimpleConnection for LoggingConnection<OciConnection> {
    fn batch_execute(&self, query: &str) -> QueryResult<()> {
        self.conn.batch_execute(query)
    }
}

impl MigrationConnection for LoggingConnection<OciConnection> {
    fn setup(&self) -> QueryResult<usize> {
        diesel::sql_query(include_str!("define_create_if_not_exists.sql")).execute(&self.conn)?;
        diesel::sql_query(include_str!("create_migration_table.sql")).execute(&self.conn)
    }
}

impl Connection for LoggingConnection<OciConnection> {
    type Backend = diesel_oci::oracle::Oracle;
    type TransactionManager = OCITransactionManager;

    fn establish(database_url: &str) -> ConnectionResult<Self> {
        let log_mode = DbLogMode::from_env();
        let conn = OciConnection::establish(database_url)?;
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

    fn transaction_manager(&self) -> &OCITransactionManager {
        self.conn.transaction_manager()
    }
}

impl R2D2Connection for LoggingConnection<OciConnection> {
    fn ping(&self) -> QueryResult<()> {
        self.conn.ping()
    }
}

impl TransactionManager<LoggingConnection<OciConnection>> for OCITransactionManager {
    fn begin_transaction(&self, conn: &LoggingConnection<OciConnection>) -> QueryResult<()> {
        self.begin_transaction(&conn.conn)
    }

    fn rollback_transaction(&self, conn: &LoggingConnection<OciConnection>) -> QueryResult<()> {
        self.rollback_transaction(&conn.conn)
    }

    fn commit_transaction(&self, conn: &LoggingConnection<OciConnection>) -> QueryResult<()> {
        self.commit_transaction(&conn.conn)
    }

    fn get_transaction_depth(&self) -> u32 {
        <OCITransactionManager as TransactionManager<OciConnection>>::get_transaction_depth(&self)
    }
}
