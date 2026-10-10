use calamine::{Data, Range, Reader, Xlsx, open_workbook};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::DataSource;
use super::error::DataError;
use crate::layout::value::{DataValue, ValueSource};

#[derive(Debug, Clone)]
pub struct ExcelDataSourceOptions {
    pub main_sheet: String,
    pub variants: Vec<String>,
}

impl ExcelDataSourceOptions {
    pub fn new(variants: Vec<String>) -> Self {
        Self {
            main_sheet: "Main".to_owned(),
            variants,
        }
    }
}

/// Excel-backed data source for variants.
pub struct ExcelDataSource {
    names: Vec<String>,
    variant_columns: Vec<Vec<Data>>,
    sheets: HashMap<String, Range<Data>>,
}

impl ExcelDataSource {
    pub fn from_path(
        path: impl AsRef<Path>,
        options: ExcelDataSourceOptions,
    ) -> Result<Self, DataError> {
        let path = path.as_ref();
        let mut workbook: Xlsx<_> = open_workbook(path).map_err(|_| {
            DataError::FileError(format!("failed to open file: {}", path.display()))
        })?;

        let main_sheet_name = options.main_sheet.as_str();
        let worksheets = workbook.worksheets();
        let main_sheet = worksheets
            .iter()
            .find_map(|(name, sheet)| (name == main_sheet_name).then_some(sheet))
            .ok_or_else(|| DataError::MiscError("Main sheet not found.".to_owned()))?;

        let rows: Vec<_> = main_sheet.rows().collect();
        let (headers, data_rows) = match rows.split_first() {
            Some((hdr, tail)) => (hdr, tail.len()),
            None => {
                return Err(DataError::RetrievalError(
                    "invalid main sheet format.".to_owned(),
                ));
            }
        };

        let name_index = headers
            .iter()
            .position(|cell| Self::cell_eq(cell, "Name"))
            .ok_or(DataError::ColumnNotFound("Name".to_owned()))?;

        let mut names: Vec<String> = Vec::with_capacity(data_rows);
        names.extend(rows.iter().skip(1).map(|row| {
            row.get(name_index)
                .map(|c| c.to_string())
                .unwrap_or_default()
        }));
        Self::validate_unique_names(&names)?;
        let variant_columns =
            Self::collect_variant_columns(headers, &rows, data_rows, &options.variants)?;

        let sheets = worksheets
            .into_iter()
            .filter(|(name, _)| name != main_sheet_name)
            .collect();

        Ok(Self {
            names,
            variant_columns,
            sheets,
        })
    }

    fn retrieve_cell(&self, name: &str) -> Result<&Data, DataError> {
        let index = self.names.iter().position(|n| n == name).ok_or_else(|| {
            DataError::RetrievalError(format!("name '{name}' not found in data sheet"))
        })?;

        for column in &self.variant_columns {
            if let Some(value) = column.get(index).filter(|v| !Self::cell_is_empty(v)) {
                return Ok(value);
            }
        }

        Err(DataError::RetrievalError(
            "data not found in any variant column".to_owned(),
        ))
    }

    fn cell_eq(cell: &Data, target: &str) -> bool {
        match cell {
            Data::String(s) => s == target,
            _ => false,
        }
    }

    fn cell_is_empty(cell: &Data) -> bool {
        match cell {
            Data::Empty => true,
            Data::String(s) => s.trim().is_empty(),
            _ => false,
        }
    }

    fn collect_column(rows: &[&[Data]], index: usize, data_rows: usize) -> Vec<Data> {
        let mut column = Vec::with_capacity(data_rows);
        column.extend(
            rows.iter()
                .skip(1)
                .map(|row| row.get(index).cloned().unwrap_or(Data::Empty)),
        );
        column
    }

    fn validate_unique_names(names: &[String]) -> Result<(), DataError> {
        let mut seen = HashSet::with_capacity(names.len());
        for name in names {
            if name.trim().is_empty() {
                continue;
            }
            if !seen.insert(name) {
                return Err(DataError::RetrievalError(format!(
                    "duplicate name '{name}' in data sheet"
                )));
            }
        }

        Ok(())
    }

    fn collect_variant_columns(
        headers: &[Data],
        rows: &[&[Data]],
        data_rows: usize,
        variants: &[String],
    ) -> Result<Vec<Vec<Data>>, DataError> {
        let mut columns = Vec::new();

        for v in variants {
            let mut matches = headers
                .iter()
                .enumerate()
                .filter(|(_, cell)| Self::cell_eq(cell, v))
                .map(|(index, _)| index);
            let index = matches
                .next()
                .ok_or_else(|| DataError::ColumnNotFound(v.clone()))?;
            if matches.next().is_some() {
                return Err(DataError::RetrievalError(format!(
                    "duplicate variant '{v}' in data sheet"
                )));
            }

            columns.push(Self::collect_column(rows, index, data_rows));
        }

        Ok(columns)
    }
}

impl DataSource for ExcelDataSource {
    fn retrieve_single_value(&self, name: &str) -> Result<DataValue, DataError> {
        DataError::while_retrieving(name, || match self.retrieve_cell(name)? {
            Data::Int(i) => Ok(DataValue::I64(*i)),
            Data::Float(f) => Ok(DataValue::F64(*f)),
            Data::Bool(b) => Ok(DataValue::Bool(*b)),
            _ => Err(DataError::RetrievalError(
                "Found non-numeric single value".to_owned(),
            )),
        })
    }

    fn retrieve_1d_array_or_string(&self, name: &str) -> Result<ValueSource, DataError> {
        DataError::while_retrieving(name, || {
            let Data::String(cell_string) = self.retrieve_cell(name)? else {
                return Err(DataError::RetrievalError(
                    "Expected string value for 1D array or string".to_owned(),
                ));
            };

            // Check if the value starts with '#' to indicate a sheet reference
            if let Some(sheet_name) = cell_string.strip_prefix('#') {
                let sheet = self.sheets.get(sheet_name).ok_or_else(|| {
                    let available: Vec<_> = self.sheets.keys().map(|s| s.as_str()).collect();
                    DataError::RetrievalError(format!(
                        "Sheet not found: '{}'. Available sheets: {}",
                        sheet_name,
                        available.join(", ")
                    ))
                })?;

                let mut out = Vec::new();

                for row in sheet.rows().skip(1) {
                    match row.first() {
                        Some(cell) if !Self::cell_is_empty(cell) => {
                            let v = match cell {
                                Data::Int(i) => DataValue::I64(*i),
                                Data::Float(f) => DataValue::F64(*f),
                                Data::Bool(b) => DataValue::Bool(*b),
                                Data::String(s) => DataValue::Str(s.to_owned()),
                                _ => {
                                    return Err(DataError::RetrievalError(
                                        "Unsupported data type in 1D array".to_owned(),
                                    ));
                                }
                            };
                            out.push(v);
                        }
                        _ => break,
                    }
                }
                return Ok(ValueSource::Array(out));
            }

            // No '#' prefix, treat as a literal string
            Ok(ValueSource::Single(DataValue::Str(cell_string.to_owned())))
        })
    }

    fn retrieve_2d_array(&self, name: &str) -> Result<Vec<Vec<DataValue>>, DataError> {
        DataError::while_retrieving(name, || {
            let Data::String(cell_string) = self.retrieve_cell(name)? else {
                return Err(DataError::RetrievalError(
                    "Expected string value for 2D array".to_owned(),
                ));
            };

            let sheet_name = cell_string.strip_prefix('#').ok_or_else(|| {
                DataError::RetrievalError(format!(
                    "2D array reference must start with '#' prefix, got: {}",
                    cell_string
                ))
            })?;

            let sheet = self.sheets.get(sheet_name).ok_or_else(|| {
                let available: Vec<_> = self.sheets.keys().map(|s| s.as_str()).collect();
                DataError::RetrievalError(format!(
                    "Sheet not found: '{}'. Available sheets: {}",
                    sheet_name,
                    available.join(", ")
                ))
            })?;

            let convert = |cell: &Data| -> Result<DataValue, DataError> {
                match cell {
                    Data::Int(i) => Ok(DataValue::I64(*i)),
                    Data::Float(f) => Ok(DataValue::F64(*f)),
                    Data::Bool(b) => Ok(DataValue::Bool(*b)),
                    _ => Err(DataError::RetrievalError(
                        "Unsupported data type in 2D array".to_owned(),
                    )),
                }
            };

            let mut rows = sheet.rows();
            let hdrs = rows.next().ok_or_else(|| {
                DataError::RetrievalError("No headers found in 2D array".to_owned())
            })?;
            let width = hdrs.iter().take_while(|c| !Self::cell_is_empty(c)).count();
            if width == 0 {
                return Err(DataError::RetrievalError(
                    "Detected zero width 2D array".to_owned(),
                ));
            }

            let mut out = Vec::new();

            for (row_index, row) in rows.enumerate() {
                if (0..width).all(|col| row.get(col).is_none_or(Self::cell_is_empty)) {
                    break;
                }

                let mut vals = Vec::with_capacity(width);
                for col in 0..width {
                    let Some(cell) = row.get(col) else {
                        return Err(DataError::RetrievalError(format!(
                            "Missing cell in 2D array at row {}, column {}",
                            row_index + 2,
                            col + 1
                        )));
                    };
                    if Self::cell_is_empty(cell) {
                        return Err(DataError::RetrievalError(format!(
                            "Empty cell in 2D array at row {}, column {}",
                            row_index + 2,
                            col + 1
                        )));
                    };
                    vals.push(convert(cell)?);
                }
                out.push(vals);
            }

            Ok(out)
        })
    }
}
