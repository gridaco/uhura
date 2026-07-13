//! Byte-offset spans and the source map that resolves them to line/col.

/// Index into the [`SourceMap`]'s file table.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FileId(pub u32);

/// A `[start, end)` byte range in one source file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(file: FileId, start: u32, end: u32) -> Self {
        debug_assert!(start <= end);
        Span { file, start, end }
    }

    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// The smallest span covering both `self` and `other` (same file).
    pub fn to(&self, other: Span) -> Span {
        debug_assert_eq!(self.file, other.file);
        Span::new(
            self.file,
            self.start.min(other.start),
            self.end.max(other.end),
        )
    }
}

/// 1-based line and character column.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

struct SourceFile {
    path: String,
    text: String,
    /// Byte offset of the start of each line; `line_starts[0] == 0`.
    line_starts: Vec<u32>,
}

/// Registered in-memory sources; pure (the CLI does the file reading).
#[derive(Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a source and returns its id. `path` is corpus-relative.
    pub fn add(&mut self, path: impl Into<String>, text: impl Into<String>) -> FileId {
        let text = text.into();
        let mut line_starts = vec![0u32];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i as u32 + 1);
            }
        }
        self.files.push(SourceFile {
            path: path.into(),
            text,
            line_starts,
        });
        FileId(self.files.len() as u32 - 1)
    }

    pub fn path(&self, file: FileId) -> &str {
        &self.files[file.0 as usize].path
    }

    pub fn text(&self, file: FileId) -> &str {
        &self.files[file.0 as usize].text
    }

    /// Resolves a byte offset to 1-based line and character column.
    pub fn line_col(&self, file: FileId, offset: u32) -> LineCol {
        let f = &self.files[file.0 as usize];
        let line_idx = match f.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let line_start = f.line_starts[line_idx] as usize;
        let upto = &f.text[line_start..(offset as usize).min(f.text.len())];
        LineCol {
            line: line_idx as u32 + 1,
            col: upto.chars().count() as u32 + 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_resolution() {
        let mut sm = SourceMap::new();
        let f = sm.add("a.uhura", "ab\ncdé f\n");
        assert_eq!(sm.line_col(f, 0), LineCol { line: 1, col: 1 });
        assert_eq!(sm.line_col(f, 3), LineCol { line: 2, col: 1 });
        // 'é' is 2 bytes; offset 7 points at ' ' after "cdé" = col 4.
        assert_eq!(sm.line_col(f, 7), LineCol { line: 2, col: 4 });
        // Offset 9 is the newline itself — still line 2. Line 3 starts at 10
        // (the EOF position).
        assert_eq!(sm.line_col(f, 9), LineCol { line: 2, col: 6 });
        assert_eq!(sm.line_col(f, 10), LineCol { line: 3, col: 1 });
    }

    #[test]
    fn span_join() {
        let f = FileId(0);
        assert_eq!(
            Span::new(f, 2, 4).to(Span::new(f, 8, 9)),
            Span::new(f, 2, 9)
        );
    }
}
