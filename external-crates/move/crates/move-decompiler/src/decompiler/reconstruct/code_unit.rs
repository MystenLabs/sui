// Copyright (c) Verichains, 2023

use std::fmt::Display;

pub(crate) enum SourceCodeItem {
    Line(String),
    Block(SourceCodeUnit),
}

pub(crate) struct SourceCodeUnit {
    indent: i32,
    code: Vec<SourceCodeItem>,
}

impl SourceCodeUnit {
    pub fn new(indent: i32) -> SourceCodeUnit {
        SourceCodeUnit {
            indent,
            code: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.code.is_empty()
    }

    pub fn add_indent(&mut self, indent: i32) {
        self.indent += indent;
    }

    pub fn add_line(&mut self, line: String) {
        self.code
            .push(SourceCodeItem::Line(line.trim_end_matches(' ').to_string()));
    }

    pub fn add_block(&mut self, block: SourceCodeUnit) {
        self.code.push(SourceCodeItem::Block(block));
    }

    pub fn print(&self, base_indent: i32, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let indent = base_indent + self.indent;

        for item in self.code.iter() {
            match item {
                SourceCodeItem::Line(line) => {
                    if !line.is_empty() {
                        for _ in 0..indent {
                            f.write_str("    ")?;
                        }
                        f.write_str(line)?;
                    }
                    f.write_str("\n")?;
                }

                SourceCodeItem::Block(block) => {
                    block.print(indent, f)?;
                }
            }
        }

        std::fmt::Result::Ok(())
    }
}

//impl Display for DecompiledCodeUnit
impl Display for SourceCodeUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.print(0, f)
    }
}
