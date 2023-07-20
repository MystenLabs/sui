// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fs, path::Path};

use move_command_line_common::{
    testing::EXP_EXT,
    testing::{add_update_baseline_fix, format_diff, read_env_update_baseline},
};
use move_compiler::{
    cfgir::visitor::AbstractInterpreterVisitor,
    command_line::compiler::move_check_for_errors,
    diagnostics::codes::{self, CategoryID, DiagnosticsID, WarningFilter},
    editions::Flavor,
    expansion::ast as E,
    shared::{NumericalAddress, PackageConfig},
    Compiler, PASS_PARSER,
};

use sui_move_build::linters::{
    custom_state_change::CustomStateChangeVerifier, known_filters,
    self_transfer::SelfTransferVerifier, share_owned::ShareOwnedVerifier, LINT_WARNING_PREFIX,
};

const SUI_FRAMEWORK_PATH: &str = "../sui-framework/packages/sui-framework";
const MOVE_STDLIB_PATH: &str = "../sui-framework/packages/move-stdlib";

fn default_testing_addresses() -> BTreeMap<String, NumericalAddress> {
    let mapping = [("std", "0x1"), ("sui", "0x2")];
    mapping
        .iter()
        .map(|(name, addr)| (name.to_string(), NumericalAddress::parse_str(addr).unwrap()))
        .collect()
}

fn linter_tests(path: &Path) -> datatest_stable::Result<()> {
    run_tests(path)?;
    Ok(())
}

pub fn known_filters_for_test() -> (E::AttributeName_, Vec<WarningFilter>) {
    let (filter_attr_name, mut filters) = known_filters();

    let unused_function_code_filter = WarningFilter::Code(
        DiagnosticsID::new(
            codes::Category::UnusedItem as u8,
            codes::UnusedItem::Function as u8,
            Some(LINT_WARNING_PREFIX),
        ),
        Some("code_suppression_should_not_work"),
    );
    let unused_function_category_filter = WarningFilter::Category(
        CategoryID::new(codes::Category::UnusedItem as u8, Some(LINT_WARNING_PREFIX)),
        Some("category_suppression_should_not_work"),
    );
    filters.push(unused_function_code_filter);
    filters.push(unused_function_category_filter);
    (filter_attr_name, filters)
}

fn run_tests(path: &Path) -> anyhow::Result<()> {
    let exp_path = path.with_extension(EXP_EXT);

    let targets: Vec<String> = vec![path.to_str().unwrap().to_owned()];
    let lint_visitors = vec![
        ShareOwnedVerifier.visitor(),
        SelfTransferVerifier.visitor(),
        CustomStateChangeVerifier.visitor(),
    ];
    let (filter_attr_name, filters) = known_filters_for_test();
    let (files, comments_and_compiler_res) = Compiler::from_files(
        targets,
        vec![MOVE_STDLIB_PATH.to_string(), SUI_FRAMEWORK_PATH.to_string()],
        default_testing_addresses(),
    )
    .add_visitors(lint_visitors)
    .set_default_config(PackageConfig {
        flavor: Flavor::Sui,
        ..PackageConfig::default()
    })
    .add_custom_known_filters(filters, filter_attr_name)
    .run::<PASS_PARSER>()?;

    let diags = move_check_for_errors(comments_and_compiler_res);

    let has_diags = !diags.is_empty();
    let diag_buffer = if has_diags {
        move_compiler::diagnostics::report_diagnostics_to_buffer(&files, diags)
    } else {
        vec![]
    };

    let update_baseline = read_env_update_baseline();

    let rendered_diags = std::str::from_utf8(&diag_buffer)?;

    if update_baseline {
        if has_diags {
            fs::write(exp_path, rendered_diags)?;
        } else if exp_path.is_file() {
            fs::remove_file(exp_path)?;
        }
        return Ok(());
    }

    let exp_exists = exp_path.is_file();
    if exp_exists {
        let expected_diags = fs::read_to_string(exp_path)?;
        if rendered_diags != expected_diags {
            let msg = format!(
                "Expected output differ from the actual one:\n{}",
                format_diff(expected_diags, rendered_diags),
            );
            anyhow::bail!(add_update_baseline_fix(msg));
        }
    } else {
        let msg = format!("Unexpected output :\n{}", rendered_diags);
        anyhow::bail!(add_update_baseline_fix(msg));
    }

    Ok(())
}

datatest_stable::harness!(linter_tests, "tests/linter", r".move$");
