use calamine::{Data, Range, Reader, Xlsx, open_workbook};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::{Arc, Mutex};

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

struct WorkbookSheets {
    workbook: Option<Xlsx<BufReader<File>>>,
    loaded: HashMap<String, Arc<Range<Data>>>,
}

/// Excel-backed data source for variants.
pub struct ExcelDataSource {
    /// Row index of each name; the first row wins for repeated blank names.
    name_rows: HashMap<String, usize>,
    variant_columns: Vec<Vec<Data>>,
    array_sheet_names: Vec<String>,
    sheets: Mutex<WorkbookSheets>,
}

impl ExcelDataSource {
    pub fn from_path(
        path: impl AsRef<Path>,
        options: ExcelDataSourceOptions,
    ) -> Result<Self, DataError> {
        let path = path.as_ref();
        let mut workbook: Xlsx<_> = open_workbook(path).map_err(|error| {
            DataError::FileError(format!("failed to open file {}: {error}", path.display()))
        })?;

        let main_sheet_name = options.main_sheet.as_str();
        let sheet_names = workbook.sheet_names();
        if !sheet_names.iter().any(|name| name == main_sheet_name) {
            return Err(DataError::MiscError(format!(
                "main sheet '{main_sheet_name}' not found. Available sheets: {}",
                sheet_names.join(", ")
            )));
        }
        let main_sheet = workbook.worksheet_range(main_sheet_name).map_err(|error| {
            DataError::FileError(format!(
                "failed to read sheet '{main_sheet_name}' from {}: {error}",
                path.display()
            ))
        })?;

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
            .ok_or_else(|| Self::missing_column("Name", main_sheet_name, headers))?;

        let mut names: Vec<String> = Vec::with_capacity(data_rows);
        names.extend(rows.iter().skip(1).map(|row| {
            row.get(name_index)
                .map(|c| c.to_string())
                .unwrap_or_default()
        }));
        Self::validate_unique_names(&names)?;
        let variant_columns = Self::collect_variant_columns(
            main_sheet_name,
            headers,
            &rows,
            data_rows,
            &options.variants,
        )?;

        let mut array_sheet_names: Vec<_> = sheet_names
            .into_iter()
            .filter(|name| name != main_sheet_name)
            .collect();
        array_sheet_names.sort_unstable();
        array_sheet_names.dedup();
        let workbook = (!array_sheet_names.is_empty()).then_some(workbook);

        let mut name_rows = HashMap::with_capacity(names.len());
        for (row, name) in names.into_iter().enumerate() {
            name_rows.entry(name).or_insert(row);
        }

        Ok(Self {
            name_rows,
            variant_columns,
            array_sheet_names,
            sheets: Mutex::new(WorkbookSheets {
                workbook,
                loaded: HashMap::new(),
            }),
        })
    }

    fn retrieve_cell(&self, name: &str) -> Result<&Data, DataError> {
        let index = *self.name_rows.get(name).ok_or_else(|| {
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

    fn array_sheet(&self, sheet_name: &str) -> Result<Arc<Range<Data>>, DataError> {
        if self
            .array_sheet_names
            .binary_search_by(|name| name.as_str().cmp(sheet_name))
            .is_err()
        {
            return Err(DataError::RetrievalError(format!(
                "Sheet not found: '{}'. Available sheets: {}",
                sheet_name,
                self.array_sheet_names.join(", ")
            )));
        }

        let mut state = self
            .sheets
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(sheet) = state.loaded.get(sheet_name) {
            return Ok(Arc::clone(sheet));
        }

        let Some(workbook) = state.workbook.as_mut() else {
            return Err(DataError::RetrievalError(format!(
                "Sheet not found: '{}'. Available sheets: {}",
                sheet_name,
                self.array_sheet_names.join(", ")
            )));
        };
        let range = workbook.worksheet_range(sheet_name).map_err(|error| {
            DataError::FileError(format!("failed to read sheet '{sheet_name}': {error}"))
        })?;
        let sheet = Arc::new(range);
        state
            .loaded
            .insert(sheet_name.to_owned(), Arc::clone(&sheet));
        if state.loaded.len() == self.array_sheet_names.len() {
            state.workbook = None;
        }
        Ok(sheet)
    }

    fn header_names(headers: &[Data]) -> String {
        headers
            .iter()
            .filter(|cell| !Self::cell_is_empty(cell))
            .map(|cell| cell.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn missing_column(column: &str, sheet_name: &str, headers: &[Data]) -> DataError {
        DataError::ColumnNotFound(format!(
            "'{column}' in sheet '{sheet_name}'. Available columns: {}",
            Self::header_names(headers)
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
        main_sheet_name: &str,
        headers: &[Data],
        rows: &[&[Data]],
        data_rows: usize,
        variants: &[String],
    ) -> Result<Vec<Vec<Data>>, DataError> {
        let mut columns = Vec::new();
        let mut seen = HashSet::new();

        for v in variants.iter().filter(|variant| seen.insert(*variant)) {
            let mut matches = headers
                .iter()
                .enumerate()
                .filter(|(_, cell)| Self::cell_eq(cell, v))
                .map(|(index, _)| index);
            let index = matches
                .next()
                .ok_or_else(|| Self::missing_column(v, main_sheet_name, headers))?;
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

            if let Some(sheet_name) = cell_string.strip_prefix('#') {
                let sheet = self.array_sheet(sheet_name)?;

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

            let sheet = self.array_sheet(sheet_name)?;

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
