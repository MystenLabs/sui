// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::PackageAnalyzerError;
use csv::Reader;
use fastcrypto::encoding::{Base64, Encoding};
use serde::Deserialize;
use std::path::PathBuf;
use sui_types::move_package::MovePackage;

#[derive(Debug, Deserialize)]
struct PackageRow {
    #[serde(rename = "PACKAGE_ID")]
    package_id: String,
    #[serde(rename = "CHECKPOINT")]
    checkpoint: u64,
    #[serde(rename = "EPOCH")]
    epoch: u64,
    #[serde(rename = "TIMESTAMP_MS")]
    timestamp_ms: u64,
    #[serde(rename = "BCS")]
    bcs: String,
    #[serde(rename = "TRANSACTION_DIGEST")]
    transaction_digest: String,
}

// Load packages from a scsv as dumped from snowflakes `move_package_parquet`
pub fn load_csv(path: PathBuf) -> Result<Vec<MovePackage>, PackageAnalyzerError> {
    let mut rdr = Reader::from_path(path).map_err(|e| {
        PackageAnalyzerError::BadCsvFile(format!("Cannot read csv file: {:?}", e))
    })?;
    let headers = rdr.headers().map_err(|e| {
        PackageAnalyzerError::BadCsvFile(format!("Cannot read csv headers: {:?}", e))
    })?;
    let mut packages = Vec::new();
    for rec in rdr.deserialize() {
        let record: PackageRow = rec.map_err(|e| {
            PackageAnalyzerError::BadCsvFile(format!("Cannot read csv record: {:?}", e))
        })?;
        let package = Base64::decode(record.bcs.as_str()).map_err(|e| {
            PackageAnalyzerError::BadCsvFile(format!("Cannot decode move package bytes: {:?}", e))
        })?;
        let move_package = bcs::from_bytes::<MovePackage>(&package).map_err(|e| {
            PackageAnalyzerError::BadCsvFile(format!("Cannot deserialize move package: {:?}", e))
        })?;
        packages.push(move_package);
    }
    Ok(packages)
}