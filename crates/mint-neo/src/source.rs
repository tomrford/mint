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

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    pub fn merge(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// One named source buffer. Header and JSON inputs both keep original text.
#[derive(Clone, Debug)]
pub struct Source {
    pub name: String,
    pub text: String,
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

    pub fn from_path(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|_| format!("failed to read file: {}", path.display()))?;
        Ok(Self::new(path.display().to_string(), text))
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn slice(&self, span: Span) -> &str {
        let start = span.start.min(self.text.len());
        let end = span.end.min(self.text.len());
        &self.text[start..end]
    }

    pub fn byte(&self, offset: usize) -> Option<u8> {
        self.text.as_bytes().get(offset).copied()
    }

    /// 1-based line and byte column for `offset`.
    pub fn locate(&self, offset: usize) -> (u32, u32) {
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

    pub fn line_text(&self, line: u32) -> &str {
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
    pub fn has_blank_line(&self, start: usize, end: usize) -> bool {
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

    pub fn only_whitespace(&self, start: usize, end: usize) -> bool {
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

#[cfg(test)]
mod tests {
    use super::{Source, Span};

    #[test]
    fn locates_line_and_column() {
        let source = Source::new("t.h", "ab\nc");
        assert_eq!(source.locate(0), (1, 1));
        assert_eq!(source.locate(3), (2, 1));
        assert_eq!(source.slice(Span::new(0, 2)), "ab");
    }

    #[test]
    fn detects_blank_lines() {
        let source = Source::new("t.h", "a\n\nb");
        assert!(source.has_blank_line(1, 3));
        assert!(!source.has_blank_line(0, 1));
    }
}
