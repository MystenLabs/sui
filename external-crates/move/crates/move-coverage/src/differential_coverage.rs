// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::lcov::PackageRecordKeeper;
use lcov::{Reader, Report, report::section as LRS};
use std::{collections::BTreeMap, fmt::Write};

/// A Differential report is one that shows the lines that are covered by both passing and failing
/// tests as "uncovered", lines only covered by the failing tests are marked as "covered", and
/// lines that are covered only by passing tests (or are not covered at all) are not marked either
/// way.
/// More explicitly:
/// * If hit in both => then uncovered in output `DA: _, _, 0`
/// * If hit in failing only => then hit in output `DA: _, _, 1`
/// * If hit in passing only (or not hit at all) => then not hit in output (_NO DA line at all_)
pub fn differential_report(
    total_record: &PackageRecordKeeper,
    test_record: &PackageRecordKeeper,
) -> anyhow::Result<String> {
    let total_record_str = total_record.lcov_record_string();
    let test_record_str = test_record.lcov_record_string();
    let total_record = Reader::new(total_record_str.as_bytes());
    let test_record = Reader::new(test_record_str.as_bytes());

    let diff_report = create_differential_report(
        &Report::from_reader(total_record)
            .map_err(|e| anyhow::anyhow!("Failed to read total report: {}", e))?,
        &Report::from_reader(test_record)
            .map_err(|e| anyhow::anyhow!("Failed to read test report: {}", e))?,
    )
    .map_err(|e| anyhow::anyhow!("Failed to create differential report: {}", e))?;

    let mut output = String::new();

    for record in diff_report.into_records() {
        writeln!(output, "{}", record)?;
    }

    Ok(output)
}

/// A Differential report is one that shows the lines that are covered by both passing and failing
/// tests as "uncovered", lines only covered by the failing tests are marked as "covered", and
/// lines that are covered only by passing tests (or are not covered at all) are not marked either
/// way.
/// More explicitly:
/// * If hit in both => then uncovered (DA: _, _, 0)
/// * If hit in failing => then hit (DA: _, _, 1)
/// * If hit in passing only (or not hit at all) => then not hit (_NO DA line_)
pub fn create_differential_report(
    total_report: &Report,
    failing_report: &Report,
) -> anyhow::Result<Report> {
    let mut resulting_report = Report::new();

    resulting_report.sections = differential_map(
        &failing_report.sections,
        &total_report.sections,
        differential_section,
    )?;
    Ok(resulting_report)
}

/// Compute the differential between two reports and return the resulting report. If the resulting
/// report is empty, return None.
pub fn differential_section(
    f_section: &LRS::Value,
    t_section: &LRS::Value,
) -> anyhow::Result<Option<LRS::Value>> {
    let lines = differential_map(&f_section.lines, &t_section.lines, differential_line)?;
    let functions = differential_map(
        &f_section.functions,
        &t_section.functions,
        differential_function,
    )?;
    let branches = differential_map(
        &f_section.branches,
        &t_section.branches,
        differential_branch,
    )?;
    if lines.is_empty() && functions.is_empty() && branches.is_empty() {
        Ok(None)
    } else {
        Ok(Some(LRS::Value {
            lines,
            functions,
            branches,
        }))
    }
}

/// Compute the differential between two lines and return the resulting line. If the resulting line
/// is not to be kept (i.e., the DA record should be removed according the rules above) return
/// `None`.
pub fn differential_line(
    f_line: &LRS::line::Value,
    t_line: &LRS::line::Value,
) -> anyhow::Result<Option<LRS::line::Value>> {
    let count = if f_line.count != 0 && t_line.count != 0 {
        // both hit -- uncovered
        Some(0)
    } else if f_line.count != 0 {
        // only f_line hit -- hit
        Some(f_line.count)
    } else {
        // remove line entry
        None
    };
    Ok(count.map(|count| LRS::line::Value {
        count,
        checksum: None,
    }))
}

/// Compute the differential between two functions and return the resulting function. If the
/// resulting function report should be removed entirely then return `None`.
pub fn differential_function(
    f_function: &LRS::function::Value,
    t_function: &LRS::function::Value,
) -> anyhow::Result<Option<LRS::function::Value>> {
    debug_assert_eq!(f_function.start_line, t_function.start_line);
    let count = if f_function.count != 0 && t_function.count != 0 {
        // both hit -- uncovered
        Some(0)
    } else if f_function.count != 0 {
        // only f_function hit -- hit
        Some(1)
    } else {
        // remove function entry
        None
    };

    Ok(count.map(|count| LRS::function::Value {
        start_line: f_function.start_line,
        count,
    }))
}

/// Compute the differential between two branches and return the resulting branch. If the BRDA
/// should be removed entirely then return `None`.
pub fn differential_branch(
    f_branch: &LRS::branch::Value,
    t_branch: &LRS::branch::Value,
) -> anyhow::Result<Option<LRS::branch::Value>> {
    let taken = if f_branch.taken.is_some() && t_branch.taken.is_some() {
        // both hit -- uncovered
        Some(None)
    } else if f_branch.taken.is_some() {
        // only f_branch hit -- hit
        Some(f_branch.taken)
    } else {
        // remove branch entry
        None
    };
    Ok(taken.map(|taken| LRS::branch::Value { taken }))
}

/// Compute the differential across a map of values. This is slightly different than a `merge_by`
/// type of operation -- more like a `merge_by_filter` type of operation --
///
/// The resulting map will contain the keys from the first map and for any values in the
/// intersection of the two maps the values will be merged with `merge_fn`. If `merge_fn` returns
/// `None` for a key, that key (and its associated values either of the two input maps) will not
/// appear in the output map at all.
pub fn differential_map<K: Clone + Ord, V: Clone>(
    f_map: &BTreeMap<K, V>,
    t_map: &BTreeMap<K, V>,
    mut merge_fn: impl FnMut(&V, &V) -> anyhow::Result<Option<V>>,
) -> anyhow::Result<BTreeMap<K, V>> {
    f_map
        .iter()
        .filter_map(|(key, value)| {
            if let Some(t_value) = t_map.get(key) {
                merge_fn(value, t_value)
                    .transpose()
                    .map(|merged_value| merged_value.map(|v| (key.clone(), v)))
            } else {
                Some(Ok((key.clone(), value.clone())))
            }
        })
        .collect()
}
