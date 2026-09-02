use std::sync::Arc;
use crate::db_value::DbValue;
use crate::{Metadata, Row};
use crate::row::RowData;

/// Trait for converting raw database response data into structured, row-oriented formats.
pub(crate) trait TransposeData {
	type Output;
	fn transpose(self, column_info: &Arc<Vec<Metadata>>) -> Self::Output;
}

// For Singleton (RowData -> Vec<Row>)

impl TransposeData for RowData {
	type Output = Vec<Row>;

	fn transpose(self, column_info: &Arc<Vec<Metadata>>) -> Vec<Row> {
		let num_cols = self.len();
		if num_cols == 0 {
			return Vec::new();
		}
		let num_rows = match self.first() {
			Some(Some(DbValue::Array(arr))) => arr.len(),
			_ => return vec![Row::new(column_info, self)]
		};

		if num_rows == 0 {
			return Vec::new();
		}
		// Unpack columnar DbValue::Array into column vectors:
		let mut columns: Vec<Vec<Option<DbValue>>> = Vec::with_capacity(num_cols);
		for col_opt in self {
			match col_opt {
				Some(DbValue::Array(arr)) => columns.push(arr),
				Some(scalar) => columns.push(vec![Some(scalar); num_rows]),
				None => columns.push(vec![None; num_rows]),
			}
		}
		// Build N horizontal Row structs:
		let mut rows = Vec::with_capacity(num_rows);
		for row_idx in 0..num_rows {
			let mut row_values = Vec::with_capacity(num_cols);
			for col in columns.iter_mut() {
				row_values.push(col[row_idx].take());
			}
			rows.push(Row::new(column_info, row_values));
		}
		rows
	}
}

// For Batch:
impl TransposeData for Vec<RowData> {
	type Output = Vec<Vec<Row>>;

	fn transpose(self, column_info: &Arc<Vec<Metadata>>) -> Vec<Vec<Row>> {
		self.into_iter()
			.map(|row_data| row_data.transpose(column_info))
			.collect()
	}
}

