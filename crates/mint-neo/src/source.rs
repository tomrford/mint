/// Byte span inside one input file.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            start: start.min(end),
            end,
        }
    }

    pub fn point(offset: usize) -> Self {
        Self::new(offset, offset)
    }
}

/// One named source buffer. Header and JSON inputs both keep original text.
#[derive(Clone, Debug)]
pub struct Source {
    pub(crate) name: String,
    pub(crate) text: String,
    line_starts: Vec<usize>,
}

impl Source {
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let line_starts = line_starts(&text);
        Self {
            name: name.into(),
            text,
            line_starts,
        }
    }

    pub fn from_path(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        Ok(Self::new(path.display().to_string(), text))
    }

    pub(crate) fn slice(&self, span: Span) -> &str {
        let start = span.start.min(self.text.len());
        let end = span.end.min(self.text.len());
        &self.text[start..end]
    }

    /// 1-based line and byte column for `offset`.
    pub(crate) fn locate(&self, offset: usize) -> (u32, u32) {
        let offset = offset.min(self.text.len());
        let line_index = match self.line_starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        };
        let line_start = self.line_starts[line_index];
        let line = u32::try_from(line_index + 1).unwrap_or(u32::MAX);
        let column = u32::try_from(offset.saturating_sub(line_start) + 1).unwrap_or(u32::MAX);
        (line, column)
    }

    pub(crate) fn line_text(&self, line: u32) -> &str {
        let index = usize::try_from(line.saturating_sub(1)).unwrap_or(0);
        let start = *self.line_starts.get(index).unwrap_or(&0);
        let end = self
            .line_starts
            .get(index + 1)
            .copied()
            .unwrap_or(self.text.len());
        let line = &self.text[start..end];
        line.strip_suffix('\n')
            .map(|value| value.strip_suffix('\r').unwrap_or(value))
            .unwrap_or(line)
    }

    /// True when `start..end` contains a blank line (two newlines with only
    /// horizontal whitespace between them).
    pub(crate) fn has_blank_line(&self, start: usize, end: usize) -> bool {
        let bytes = self.text.as_bytes();
        let start = start.min(bytes.len());
        let end = end.min(bytes.len());
        let mut prev_nl = false;
        let mut only_ws = true;
        for &byte in &bytes[start..end] {
            match byte {
                b'\n' => {
                    if prev_nl && only_ws {
                        return true;
                    }
                    prev_nl = true;
                    only_ws = true;
                }
                b'\r' => {}
                b' ' | b'\t' => {}
                _ => {
                    prev_nl = false;
                    only_ws = false;
                }
            }
        }
        false
    }

    pub(crate) fn only_whitespace(&self, start: usize, end: usize) -> bool {
        self.text
            .get(start.min(self.text.len())..end.min(self.text.len()))
            .is_some_and(|text| text.bytes().all(|byte| byte.is_ascii_whitespace()))
    }
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}
