use std::sync::Arc;
use crate::db_value::DbValue;
use crate::{Metadata, Row};
use crate::row::RowData;

/// Opaque wrapper ensuring a statement's raw columnar wire container
/// MUST be transposed before it can be converted into `Row` structs.
pub(crate) struct RawColumnarData(RowData);

impl RawColumnarData {
	/// Creates a new opaque wrapper around a statement's raw wire container.
	pub(crate) fn new(data: RowData) -> Self {
		Self(data)
	}
}

/// Trait for converting raw database response data into structured, row-oriented formats.
pub(crate) trait TransposeData {
	type Output;
	fn transpose(self, column_info: &Arc<Vec<Metadata>>) -> Self::Output;
}

// For Singleton (RawColumnarData -> Vec<Row>)
impl TransposeData for RawColumnarData {
	type Output = Vec<Row>;

	fn transpose(self, column_info: &Arc<Vec<Metadata>>) -> Vec<Row> {
		let container_row = self.0;
		let num_cols = container_row.len();
		if num_cols == 0 {
			return Vec::new();
		}

		// 1. Pre-flight Matrix Invariant Check:
		let lengths: Vec<usize> = container_row
			.iter()
			.filter_map(|col| match col {
				Some(DbValue::Array(arr)) => Some(arr.len()),
				_ => None,
			})
			.collect();

		let num_rows = match lengths.first() {
			None => return vec![Row::new(column_info, container_row)],
			Some(&len) if lengths.iter().all(|&l| l == len) => len,
			Some(_) => panic!("Database returned ragged columnar data with mismatched row counts!"),
		};

		if num_rows == 0 {
			return Vec::new();
		}

		// 2. Unpack columns:
		let mut columns: Vec<Vec<Option<DbValue>>> = Vec::with_capacity(num_cols);
		for col_opt in container_row {
			match col_opt {
				Some(DbValue::Array(arr)) => columns.push(arr),
				Some(scalar) => columns.push(vec![Some(scalar); num_rows]),
				None => columns.push(vec![None; num_rows]),
			}
		}

		// 3. Fast Pointer Lockstep Transposition:
		let mut iters: Vec<_> = columns.iter_mut().map(|c| c.iter_mut()).collect();
		let mut rows = Vec::with_capacity(num_rows);
		for _ in 0..num_rows {
			let mut row_values = Vec::with_capacity(num_cols);
			for it in iters.iter_mut() {
				row_values.push(it.next().and_then(|opt| opt.take()));
			}
			rows.push(Row::new(column_info, row_values));
		}

		rows
	}
}

// For Batch (Vec<RawColumnarData> -> Vec<Vec<Row>>)
impl TransposeData for Vec<RawColumnarData> {
	type Output = Vec<Vec<Row>>;

	fn transpose(self, column_info: &Arc<Vec<Metadata>>) -> Vec<Vec<Row>> {
		self.into_iter()
			.map(|raw_data| raw_data.transpose(column_info))
			.collect()
	}
}

