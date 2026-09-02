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

use crate::metadata::Metadata;
use crate::response::Response;
use crate::row::{Row, RowData};
use crate::transpose::TransposeData;

/// Represents the result returned by the database when calling
/// [Connection::execute()](`crate::Connection::execute()`),
/// [Connection::execute_named()](`crate::Connection::execute_named()`), or
/// [Connection::execute_batch()](`crate::Connection::execute_batch()`).
pub struct ExecResult {
    column_info: Arc<Vec<Metadata>>,
    returned_data: Option<Vec<RowData>>,
    rows_affected: u64,
}

pub struct ExecBatchResult {
	column_info: Arc<Vec<Metadata>>,
	returned_data: Option<Vec<RowData>>,
	rows_affected: u64,
}

impl ExecResult {
    pub(crate) fn new(
        column_info: &[Metadata],
        resp: &mut Response,
    ) -> ExecResult {
        ExecResult {
            column_info: Arc::new(column_info.to_vec()),
            returned_data: resp.take_rows(),
            rows_affected: resp.get_rowcount(),
        }
    }

    /// Returns the number of rows affected by the execution of the statement.
    pub fn rows_affected(&self) -> u64 {
        self.rows_affected
    }

    /// Returns data returned by the database as OUT variables (PL/SQL or
    /// RETURNING statements. This transfers ownership of the returned data to
    /// the caller.
    pub fn returned_data(&mut self) -> Vec<Row> {
        if let Some(mut returned_data) = self.returned_data.take() {
           if let Some(row_data) = returned_data.pop() {
	           row_data.transpose(&self.column_info)
           } else {
	           Vec::new()
           }
        } else {
            Vec::<Row>::new()
        }
    }
}

impl ExecBatchResult {
	pub(crate) fn new(column_info: &[Metadata], resp: &mut Response) -> ExecBatchResult {
		ExecBatchResult {
			column_info: Arc::new(column_info.to_vec()),
			returned_data: resp.take_rows(),
			rows_affected: resp.get_rowcount(),
		}
	}
	pub fn rows_affected(&self) -> u64 {
		self.rows_affected
	}

	/// Returns data returned by the database as OUT variables (PL/SQL or
	/// RETURNING statements) for each execution in the batch.
	pub fn returned_data(&mut self) -> Vec<Vec<Row>> {
		if let Some(returned_data) = self.returned_data.take() {
			returned_data.transpose(&self.column_info)
		} else {
			Vec::<Vec<Row>>::new()
		}
	}
}
