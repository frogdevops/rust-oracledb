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
// Defines the structure representing execution results.
//-----------------------------------------------------------------------------

use std::sync::Arc;

use crate::error::Error;
use crate::metadata::Metadata;
use crate::response::Response;
use crate::row::Row;
use crate::transpose::{RawColumnarData, TransposeData};

/// Represents the result returned by the database when calling
/// [Connection::execute()](`crate::Connection::execute()`) or
/// [Connection::execute_named()](`crate::Connection::execute_named()`).
pub struct ExecResult {
    column_info: Arc<Vec<Metadata>>,
    returned_data: Option<RawColumnarData>,
    rows_affected: u64,
}

/// Represents the result returned by the database when calling
/// [Connection::execute_batch()](`crate::Connection::execute_batch()`) or
/// [Statement::execute_batch()](`crate::Statement::execute_batch()`).
pub struct ExecBatchResult {
    column_info: Arc<Vec<Metadata>>,
    returned_data: Option<Vec<RawColumnarData>>,
    rows_affected: u64,
}

impl ExecResult {
    pub(crate) fn new(
        column_info: &[Metadata],
        resp: &mut Response,
    ) -> ExecResult {
        ExecResult {
            column_info: Arc::new(column_info.to_vec()),
            returned_data: resp
                .take_rows()
                .and_then(|mut v| v.pop())
                .map(RawColumnarData::new),
            rows_affected: resp.get_rowcount(),
        }
    }

    /// Returns the number of rows affected by the execution of the statement.
    pub fn rows_affected(&self) -> u64 {
        self.rows_affected
    }

    /// Returns data returned by the database as OUT variables (PL/SQL or
    /// RETURNING statements). This transfers ownership of the returned data to
    /// the caller.
    pub fn returned_data(&mut self) -> Result<Vec<Row>, Error> {
        if let Some(raw_data) = self.returned_data.take() {
            raw_data.transpose(&self.column_info)
        } else {
            Ok(Vec::new())
        }
    }

    /// Returns the single row returned by the database as OUT variables
    /// (PL/SQL or RETURNING statements). If no rows were returned, a
    /// NoDataFound error is returned instead. If more than 1 row was returned,
    /// an OutOfRange error is returned. This transfers ownership of the
    /// returned data to the caller.
    pub fn returned_row(&mut self) -> Result<Row, Error> {
        let rows = self.returned_data()?;
        match rows.len() {
            0 => Err(Error::no_data_found()),
            1 => Ok(rows.into_iter().next().unwrap()),
            n => Err(Error::out_of_range(format!(
                "expected exactly 1 returned row, but found {}",
                n
            ))),
        }
    }
}

impl ExecBatchResult {
    pub(crate) fn new(
        column_info: &[Metadata],
        resp: &mut Response,
    ) -> ExecBatchResult {
        ExecBatchResult {
            column_info: Arc::new(column_info.to_vec()),
            returned_data: resp
                .take_rows()
                .map(|rows| rows.into_iter().map(RawColumnarData::new).collect()),
            rows_affected: resp.get_rowcount(),
        }
    }

    /// Returns the total number of rows affected by the execution of the batch.
    pub fn rows_affected(&self) -> u64 {
        self.rows_affected
    }

    /// Returns data returned by the database as OUT variables (PL/SQL or
    /// RETURNING statements) for each execution in the batch. This transfers
    /// ownership of the returned data to the caller.
    pub fn returned_data(&mut self) -> Result<Vec<Vec<Row>>, Error> {
        if let Some(batch_data) = self.returned_data.take() {
            batch_data.transpose(&self.column_info)
        } else {
            Ok(Vec::new())
        }
    }
}
