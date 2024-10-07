// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use crate::cli::notion::models::properties::{FormulaResultValue, PropertyValue};

    #[test]
    fn parse_number_formula_prop() {
        let _property: PropertyValue =
            serde_json::from_str(include_str!("tests/formula_number_value.json")).unwrap();
    }

    #[test]
    fn parse_date_formula_prop() {
        let _property: PropertyValue =
            serde_json::from_str(include_str!("tests/formula_date_value.json")).unwrap();
    }

    #[test]
    fn parse_number_formula() {
        let _value: FormulaResultValue = serde_json::from_str(
            r#"{
    "type": "number",
    "number": 0
  }"#,
        )
        .unwrap();
    }
}
