// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{AccumulatedDisplayData, DisplayEntry};
use anyhow::Result;
use std::path::PathBuf;
use tempfile::tempdir;

fn create_dummy_display_entry(idx: u8) -> DisplayEntry {
    DisplayEntry {
        object_type: vec![0x01, 0x02, 0x03, idx],
        display_id: vec![0x04, 0x05, 0x06, idx],
        display_version: 1,
        display: vec![0x07, 0x08, 0x09, idx],
    }
}

fn create_dummy_entries(count: u8) -> Vec<DisplayEntry> {
    (0..count).map(create_dummy_display_entry).collect()
}

fn write_entries_to_csv(entries: &[DisplayEntry], path: &PathBuf) -> Result<()> {
    let mut writer = csv::WriterBuilder::new()
        .has_headers(true)
        .from_path(path)?;

    writer.write_record(["object_type", "display_id", "display_version", "display"])?;

    for entry in entries {
        let object_type_hex = hex::encode(&entry.object_type);
        let display_id_hex = hex::encode(&entry.display_id);
        let display_hex = hex::encode(&entry.display);

        writer.write_record([
            &format!("\\x{}", object_type_hex),
            &format!("\\x{}", display_id_hex),
            &entry.display_version.to_string(),
            &format!("\\x{}", display_hex),
        ])?;
    }

    writer.flush()?;
    Ok(())
}

fn extract_entries(data: &AccumulatedDisplayData) -> Vec<DisplayEntry> {
    data.displays.values().cloned().collect()
}

fn compare_entries(a: &[DisplayEntry], b: &[DisplayEntry]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut matched_b = vec![false; b.len()];
    for entry_a in a {
        let mut found_match = false;

        for (idx_b, entry_b) in b.iter().enumerate() {
            if matched_b[idx_b] {
                continue; // Skip already matched entries
            }

            if entry_a.object_type == entry_b.object_type
                && entry_a.display_id == entry_b.display_id
                && entry_a.display_version == entry_b.display_version
                && entry_a.display == entry_b.display
            {
                matched_b[idx_b] = true;
                found_match = true;
                break;
            }
        }

        if !found_match {
            return false;
        }
    }

    true
}

struct TestSetup {
    temp_dir: tempfile::TempDir,
    file_path: String,
}

impl TestSetup {
    async fn new(entries: &[DisplayEntry]) -> Result<Self> {
        let temp_dir = tempdir()?;
        let file_name = "displays_10_233333.csv";
        let csv_path = temp_dir.path().join(file_name);
        write_entries_to_csv(entries, &csv_path)?;
        Ok(Self {
            temp_dir,
            file_path: file_name.to_string(),
        })
    }

    fn temp_dir_path(&self) -> &std::path::Path {
        self.temp_dir.path()
    }
}

#[tokio::test]
async fn test_load_display_entries_from_csv() -> Result<()> {
    let dummy_entries = create_dummy_entries(5);
    let setup = TestSetup::new(&dummy_entries).await?;
    let file_path = format!("{}/{}", setup.temp_dir_path().display(), setup.file_path);

    let mut loaded_entries = Vec::new();
    let file_content = std::fs::read_to_string(&file_path)?;
    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(file_content.as_bytes());

    for result in csv_reader.records() {
        let record = result?;

        if record.len() < 4 {
            continue;
        }

        let parse_hex = |hex: &str| -> Result<Vec<u8>> {
            let hex = hex.trim_start_matches("\\x");
            Ok(hex::decode(hex)?)
        };

        let object_type = parse_hex(&record[0])?;
        let display_id = parse_hex(&record[1])?;
        let display_version = record[2].parse::<i16>()?;
        let display = parse_hex(&record[3])?;

        let entry = DisplayEntry {
            object_type,
            display_id,
            display_version,
            display,
        };

        loaded_entries.push(entry);
    }

    assert!(
        compare_entries(&dummy_entries, &loaded_entries),
        "Manually loaded entries don't match original entries"
    );
    let mut epoch_data = AccumulatedDisplayData::new(10);
    epoch_data.update_displays(loaded_entries);
    let extracted_entries = extract_entries(&epoch_data);
    assert!(
        compare_entries(&dummy_entries, &extracted_entries),
        "Entries in AccumulatedDisplayData don't match original entries"
    );
    setup.temp_dir.close()?;

    Ok(())
}
