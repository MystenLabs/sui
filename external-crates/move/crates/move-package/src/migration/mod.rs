// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    io::{BufRead, Write},
};

use move_command_line_common::interactive::Terminal;
use move_compiler::{diagnostics::Migration, editions::Edition};
use once_cell::sync::Lazy;

use crate::compilation::build_plan::BuildPlan;

pub const MIGRATION_MSG: &str =
    "Package toml does not specify an edition. As of 2024, Move requires all packages to define \
    a language edition.";

pub const EDITION_SELECT_PROMPT: &str = "Please select one of the following editions:";

pub static EDITION_OPTIONS: Lazy<BTreeMap<String, Edition>> = Lazy::new(|| {
    let mut map = BTreeMap::new();
    map.insert("1".to_string(), Edition::E2024);
    map.insert("2".to_string(), Edition::LEGACY);
    map
});

pub const EDITION_RECORDED_MSG: &str = "Recorded edition in 'Move.toml'";

pub const MIGRATION_PROMPT: &str =
    "Would you like the Move compiler to migrate your code to Move 2024?";

pub const NOMIGRATION_HELP_MSG: &str = "No migration was performed.";

pub const MIGRATION_RERUN: &str = "You can rerun this migration by calling 'move migrate' \
    or removing the edition from your package's 'Move.toml' file and rerun 'move build'.";

pub const MIGRATION_DIFF_START_MSG: &str = "Generated changes . . .";

pub const MIGRATION_DIFF_MSG: &str = "The following changes will be made.";

pub const MIGRATION_CONFIRM_PROMPT: &str = "Would you like to apply these changes now?";

pub const APPLY_MIGRATION_PATCH_PROMPT: &str = "Apply changes?";

pub const WROTE_PATCHFILE: &str = "Wrote patchfile out to: ";

pub const NO_MIGRATION_NEEDED_MSG: &str = "No migration is required. Enjoy!";

pub const BAR: &str = "============================================================";

pub struct MigrationContext<'a, W: Write, R: BufRead> {
    build_plan: BuildPlan,
    terminal: Terminal<'a, W, R>,
}

pub struct MigrationOptions {
    pub edition: Edition,
}

pub fn migrate<W: Write, R: BufRead>(
    build_plan: BuildPlan,
    writer: &mut W,
    reader: &mut R,
) -> anyhow::Result<MigrationOptions> {
    let mut mcontext = MigrationContext::new(build_plan, writer, reader);
    mcontext.prompt_for_migration()
}

impl<'a, W: Write, R: BufRead> MigrationContext<'a, W, R> {
    pub fn new<'new>(
        build_plan: BuildPlan,
        writer: &'new mut W,
        reader: &'new mut R,
    ) -> MigrationContext<'new, W, R> {
        let terminal = Terminal::new(writer, reader);
        MigrationContext {
            build_plan,
            terminal,
        }
    }

    pub fn prompt_for_migration(&mut self) -> anyhow::Result<MigrationOptions> {
        self.terminal.writeln(MIGRATION_MSG)?;
        self.terminal.newline()?;
        let edition = self.select_edition()?;

        match edition {
            Edition::LEGACY => {
                self.terminal.newline()?;
                self.terminal.newline()?;
                self.terminal.writeln(MIGRATION_RERUN)?;
                self.terminal.newline()?;
                self.build_plan.record_package_edition(edition)?;
                self.terminal.writeln(EDITION_RECORDED_MSG)?;
                self.terminal.newline()?;
            }
            Edition::E2024 => {
                self.terminal.newline()?;
                self.terminal.newline()?;
                self.perform_upgrade()?;
                self.build_plan.record_package_edition(edition)?;
                self.terminal.writeln(EDITION_RECORDED_MSG)?;
            }
            _ => unreachable!(),
        }

        Ok(MigrationOptions { edition })
    }

    fn select_edition(&mut self) -> anyhow::Result<Edition> {
        self.terminal
            .option_prompt(EDITION_SELECT_PROMPT, &EDITION_OPTIONS)
    }

    fn perform_upgrade(&mut self) -> anyhow::Result<()> {
        if self.terminal.yes_no_prompt(MIGRATION_PROMPT, true)? {
            self.terminal.newline()?;
            self.terminal.writeln(MIGRATION_DIFF_START_MSG)?;
            let migration = self.build_plan.migrate(self.terminal.writer)?;
            self.terminal.newline()?;
            if let Some(migration) = migration {
                self.perform_upgrade_migration(migration)
            } else {
                self.terminal.writeln(NO_MIGRATION_NEEDED_MSG)
            }
        } else {
            self.terminal.writeln(NOMIGRATION_HELP_MSG)?;
            self.terminal.writeln(MIGRATION_RERUN)
        }
    }

    pub fn perform_upgrade_migration(&mut self, mut migration: Migration) -> anyhow::Result<()> {
        self.terminal.writeln(MIGRATION_DIFF_MSG)?;
        self.terminal.writeln(BAR)?;
        self.terminal.newline()?;
        self.terminal.writeln(&migration.render_output())?;
        self.terminal.newline()?;
        self.terminal.writeln(BAR)?;
        let apply = self
            .terminal
            .yes_no_prompt(APPLY_MIGRATION_PATCH_PROMPT, true)?;
        if apply {
            migration.apply_changes(self.terminal.writer)?;
            self.terminal.newline()?;
            self.terminal.writeln("Changes complete")?;
        }

        let filename = migration.record_diff(self.build_plan.root_package_path())?;
        self.terminal.write(WROTE_PATCHFILE)?;
        self.terminal.writeln(filename.as_str())?;
        self.terminal.newline()?;

        if !apply {
            self.terminal.writeln(NOMIGRATION_HELP_MSG)?;
            self.terminal.writeln(MIGRATION_RERUN)?;
        }
        Ok(())
    }
}
