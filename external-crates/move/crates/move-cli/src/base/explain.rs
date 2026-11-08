// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;

#[derive(Parser)]
#[clap(name = "explain")]
pub struct Explain {
    /// Diagnostic code, for example `EC01001` or `WSL02001`.
    #[clap(value_name = "CODE", required_unless_present = "list")]
    pub code: Option<String>,

    /// List every compiler and linter diagnostic code.
    #[clap(long, conflicts_with = "code")]
    pub list: bool,
}

impl Explain {
    pub fn execute(self) -> anyhow::Result<()> {
        if self.list {
            print!("{}", move_compiler::diagnostics::explain::DiagnosticIndex);
            return Ok(());
        }

        let code = self.code.expect("clap requires CODE unless --list is used");
        match move_compiler::diagnostics::explain::find_explanation(&code) {
            Ok(explanation) => {
                print!("{explanation}");
                Ok(())
            }
            Err(error) => anyhow::bail!("{error}"),
        }
    }
}
