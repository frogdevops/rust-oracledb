//-----------------------------------------------------------------------------
// Copyright (c) 2026, Oracle and/or its affiliates.
//
// This software is dual-licensed to you under the Universal Permissive License
// (UPL) 1.0 as shown at https://oss.oracle.com/licenses/upl and Apache License
// 2.0 as shown at http://www.apache.org/licenses/LICENSE-2.0. You may choose
// either license.
//
// If you elect to accept the software under the Apache License, Version 2.0,
// the following applies:
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//-----------------------------------------------------------------------------

//-----------------------------------------------------------------------------
// exec_result.rs
//
// Defines the structures representing execution results.
//-----------------------------------------------------------------------------

use std::sync::Arc;

use crate::error::Error;
use crate::metadata::Metadata;
use crate::response::Response;
use crate::row::{DbRow, Row};
use crate::statement::CachedStatement;
use crate::transpose::{RawColumnarData, TransposeData};

// Keep wire containers until extraction to validate their count.
enum ExecutionOutput {
    None,
    PlSql(Vec<DbRow>),
    DmlReturning(Vec<DbRow>),
}
pub (crate) type ReturnedRows = Vec<Row>;
pub(crate) type BatchReturnedRows = Vec<ReturnedRows>;
impl ExecutionOutput {
    fn new(statement: &CachedStatement, resp: &mut Response) -> Self {
        let rows = resp.take_rows();
        if statement.out_metadata().is_empty() {
            return Self::None;
        }
        match rows {
            Some(rows) if statement.is_plsql() => Self::PlSql(rows),
            Some(rows) if statement.is_dml_returning() => {
                Self::DmlReturning(rows)
            }
            _ => Self::None,
        }
    }

    fn into_rows(
        self,
        plsql: bool,
        count: usize,
    ) -> Result<Option<Vec<DbRow>>, Error> {
        let rows = match (self, plsql) {
            (Self::None, _) => return Ok(None),
            (Self::PlSql(rows), true) | (Self::DmlReturning(rows), false) => {
                rows
            }
            (Self::PlSql(_), false) => {
                return Err(Error::output_kind_mismatch(
                    "DML RETURNING",
                    "PL/SQL",
                ));
            }
            (Self::DmlReturning(_), true) => {
                return Err(Error::output_kind_mismatch(
                    "PL/SQL",
                    "DML RETURNING",
                ));
            }
        };
        if rows.len() != count {
            return Err(Error::unexpected_result());
        }
        Ok(Some(rows))
    }
}

/// Result of a single execution. Read counts before consuming output.
/// Extraction consumes the result even on error. Absence is None; SQL NULL
/// is represented by an optional value within an existing row.
///
/// ```
/// fn extract_once(result: oracledb::ExecResult) -> Result<(), oracledb::Error> {
///     let affected = result.rows_affected();
///     if let Some(row) = result.into_out_bind_data()? {
///         let value: Option<String> = row.get(0)?;
///     }
///     Ok(())
/// }
/// ```
///
/// ```compile_fail,E0382
/// fn extract_twice(result: oracledb::ExecResult) {
///     let _ = result.into_out_bind_data();
///     let _ = result.into_out_bind_data();
/// }
/// ```
pub struct ExecResult {
    column_info: Arc<Vec<Metadata>>,
    output: ExecutionOutput,
    rows_affected: u64,
}

impl ExecResult {
    pub(crate) fn new(
        statement: &CachedStatement,
        resp: &mut Response,
    ) -> Self {
        Self {
            column_info: Arc::new(statement.out_metadata().to_vec()),
            output: ExecutionOutput::new(statement, resp),
            rows_affected: resp.get_rowcount(),
        }
    }

    /// Returns the affected-row count. Call before consuming the result.
    pub fn rows_affected(&self) -> u64 {
        self.rows_affected
    }

    /// Consumes PL/SQL OUT data, retaining array-valued columns.
    /// Returns None if no container was supplied. Existing DML RETURNING
    /// output causes an ExecutionOutputKindMismatch error.
    pub fn into_out_bind_data(self) -> Result<Option<Row>, Error> {
        Ok(self
            .output
            .into_rows(true, 1)?
            .map(|mut rows| Row::new(&self.column_info, rows.pop().unwrap())))
    }

    /// Consumes and transposes DML RETURNING data.
    /// None means no container was supplied; Some(vec![]) means a supplied
    /// container contained zero rows. PL/SQL output causes an
    /// ExecutionOutputKindMismatch error; malformed containers return an error.
    pub fn into_returned_data(self) -> Result<Option<ReturnedRows>, Error> {
        self.output
            .into_rows(false, 1)?
            .map(|mut rows| {
                RawColumnarData::new(rows.pop().unwrap())
                    .transpose(&self.column_info)
            })
            .transpose()
    }

    /// Consumes exactly one DML RETURNING row. Absence or zero rows produces
    /// NoDataFound; multiple rows produce OutOfRange.
    pub fn into_returned_row(self) -> Result<Row, Error> {
        let rows = self
            .into_returned_data()?
            .ok_or_else(Error::no_data_found)?;
        match rows.len() {
            0 => Err(Error::no_data_found()),
            1 => Ok(rows.into_iter().next().unwrap()),
            n => Err(Error::out_of_range(format!(
                "expected exactly 1 returned row, but found {n}"
            ))),
        }
    }
}

/// Result of batch execution. Extraction consumes the result.
/// Present output must contain one container per execution. Empty DML groups
/// retain their position; missing containers produce an error.
///
/// ```compile_fail,E0382
/// fn extract_twice(result: oracledb::ExecBatchResult) {
///     let _ = result.into_returned_data();
///     let _ = result.into_returned_data();
/// }
/// ```
pub struct ExecBatchResult {
    column_info: Arc<Vec<Metadata>>,
    output: ExecutionOutput,
    num_execs: usize,
    rows_affected: u64,
}

impl ExecBatchResult {
    pub(crate) fn new(
        statement: &CachedStatement,
        num_execs: usize,
        resp: &mut Response,
    ) -> Self {
        Self {
            column_info: Arc::new(statement.out_metadata().to_vec()),
            output: ExecutionOutput::new(statement, resp),
            num_execs,
            rows_affected: resp.get_rowcount(),
        }
    }

    /// Returns the total affected-row count before extraction.
    pub fn rows_affected(&self) -> u64 {
        self.rows_affected
    }

    /// Consumes PL/SQL OUT rows in execution order without transposition.
    /// Returns None for absent output, or an error for a wrong output kind or
    /// an unexpected container count.
    pub fn into_out_bind_data(self) -> Result<Option<ReturnedRows>, Error> {
        Ok(self.output.into_rows(true, self.num_execs)?.map(|rows| {
            rows.into_iter()
                .map(|row| Row::new(&self.column_info, row))
                .collect()
        }))
    }

    /// Consumes DML RETURNING rows grouped in execution order.
    /// Empty groups are retained. Returns None for absent output, or an error
    /// for a wrong output kind, malformed data, or missing execution containers.
    pub fn into_returned_data(self) -> Result<Option<BatchReturnedRows>, Error> {
        self.output
            .into_rows(false, self.num_execs)?
            .map(|rows| {
                rows.into_iter()
                    .map(|row| {
                        RawColumnarData::new(row).transpose(&self.column_info)
                    })
                    .collect()
            })
            .transpose()
    }
}
