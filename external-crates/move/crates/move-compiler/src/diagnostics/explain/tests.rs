// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::collections::HashSet;

#[test]
fn diagnostic_ids_are_unique() {
    let mut ids = HashSet::new();
    for diagnostic in all_diagnostics() {
        assert!(
            ids.insert(diagnostic.info.id()),
            "duplicate diagnostic id {}",
            rendered_code(&diagnostic.info),
        );
    }
}

#[test]
fn every_lint_has_a_detailed_explanation() {
    let missing: Vec<_> = MOVE_LINTS
        .iter()
        .chain(SUI_LINTS.iter())
        .filter(|diagnostic| diagnostic.explanation.is_none())
        .map(|diagnostic| rendered_code(&diagnostic.info))
        .collect();
    assert!(
        missing.is_empty(),
        "lints without an explanation markdown file: {missing:?}"
    );
}

#[test]
fn finds_compiler_diagnostics_by_code() {
    let diagnostic = &COMPILER_DIAGNOSTICS[0];
    let code = rendered_code(&diagnostic.info);
    assert_eq!(
        find_explanation(&code).unwrap().diagnostic.info.id(),
        diagnostic.info.id(),
    );
    assert_eq!(
        find_explanation(&format!("error[{code}]"))
            .unwrap()
            .diagnostic
            .info
            .id(),
        diagnostic.info.id(),
    );
    assert_eq!(
        find_explanation(&code.to_ascii_lowercase())
            .unwrap()
            .diagnostic
            .info
            .id(),
        diagnostic.info.id(),
    );
}

#[test]
fn rejects_names_and_unknown_codes() {
    for query in [
        "InvalidAddress",
        "Syntax::InvalidAddress",
        "share_owned",
        "error[İ]",
    ] {
        assert!(
            find_explanation(query).is_err(),
            "unexpected match for {query}"
        );
    }
}

#[test]
fn finds_move_and_sui_lints_by_code() {
    for diagnostic in [&MOVE_LINTS[0], &SUI_LINTS[0]] {
        let code = rendered_code(&diagnostic.info);
        assert_eq!(
            find_explanation(&code).unwrap().diagnostic.info.id(),
            diagnostic.info.id(),
        );
        let elevated = format!("E{}", &code[1..]);
        assert_eq!(
            find_explanation(&elevated).unwrap().diagnostic.info.id(),
            diagnostic.info.id(),
        );
    }
}

#[test]
fn finds_sui_compiler_diagnostics_by_code() {
    let diagnostic = &SUI_COMPILER_DIAGNOSTICS[0];
    let code = rendered_code(&diagnostic.info);
    assert_eq!(
        find_explanation(&code).unwrap().diagnostic.info.id(),
        diagnostic.info.id(),
    );
}

#[test]
fn compiler_diagnostics_without_markdown_still_render() {
    let diagnostic = COMPILER_DIAGNOSTICS
        .iter()
        .find(|diagnostic| diagnostic.explanation.is_none())
        .unwrap();
    let code = rendered_code(&diagnostic.info);
    let rendered = find_explanation(&code).unwrap().to_string();
    assert!(rendered.contains(diagnostic.info.message()));
    assert!(rendered.contains("No detailed explanation is available"));
}

#[test]
fn index_lists_compiler_and_linter_codes() {
    let index = DiagnosticIndex.to_string();
    for heading in ["Move compiler", "Move lints", "Sui compiler", "Sui lints"] {
        assert!(index.contains(heading));
    }
    for diagnostic in [
        &COMPILER_DIAGNOSTICS[0],
        &MOVE_LINTS[0],
        &SUI_COMPILER_DIAGNOSTICS[0],
        &SUI_LINTS[0],
    ] {
        assert!(index.contains(&rendered_code(&diagnostic.info)));
        assert!(index.contains(diagnostic.info.message()));
    }
}
