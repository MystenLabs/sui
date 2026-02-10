// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use prost::Message;
use sui_config::node::CongestionLogConfig;

use crate::consensus_handler::ConsensusCommitInfo;

use super::shared_object_congestion_tracker::FinishedCommitData;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/sui.logs.rs"));
}

pub struct CongestionCommitLogger {
    base_path: PathBuf,
    max_file_size: u64,
    max_files: u32,
    current_file: File,
    current_file_size: u64,
    current_suffix: u64,
}

impl CongestionCommitLogger {
    pub fn new(config: &CongestionLogConfig) -> io::Result<Self> {
        // Always start a fresh file on startup to avoid appending
        // after a potentially corrupt partial write from a crash.
        let current_suffix = Self::existing_suffixes(&config.path)
            .into_iter()
            .max()
            .map_or(0, |s| s + 1);

        let current_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(Self::file_path_for(&config.path, current_suffix))?;

        let logger = Self {
            base_path: config.path.clone(),
            max_file_size: config.max_file_size,
            max_files: config.max_files,
            current_file,
            current_file_size: 0,
            current_suffix,
        };
        logger.delete_excess_files();
        Ok(logger)
    }

    fn file_path_for(base_path: &Path, suffix: u64) -> PathBuf {
        let mut path = base_path.as_os_str().to_owned();
        path.push(format!(".{suffix}"));
        PathBuf::from(path)
    }

    fn existing_suffixes(base_path: &Path) -> Vec<u64> {
        let (Some(dir), Some(stem)) = (base_path.parent(), base_path.file_name()) else {
            return Vec::new();
        };
        let mut prefix = stem.to_owned();
        prefix.push(".");
        let prefix = prefix.to_string_lossy().into_owned();

        fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                e.file_name()
                    .to_string_lossy()
                    .strip_prefix(&prefix)?
                    .parse()
                    .ok()
            })
            .collect()
    }

    fn delete_excess_files(&self) {
        let Some(cutoff) = self.current_suffix.checked_sub(self.max_files as u64) else {
            return;
        };
        for n in Self::existing_suffixes(&self.base_path) {
            if n <= cutoff {
                let _ = fs::remove_file(Self::file_path_for(&self.base_path, n));
            }
        }
    }

    pub fn write_commit_log(
        &mut self,
        epoch: u64,
        commit_info: &ConsensusCommitInfo,
        for_randomness: bool,
        data: &FinishedCommitData,
    ) {
        let log = proto::CongestionCommitLog {
            epoch,
            round: commit_info.round,
            timestamp_ms: commit_info.timestamp,
            commit_budget: data.commit_budget,
            for_randomness,
            final_object_execution_costs: data
                .final_object_execution_costs
                .iter()
                .map(|(id, cost)| proto::ObjectCost {
                    object_id: id.to_vec(),
                    cost: *cost,
                })
                .collect(),
            transaction_entries: data
                .log_entries
                .iter()
                .map(|entry| proto::TransactionCostEntry {
                    tx_digest: entry.tx_digest.into_inner().to_vec(),
                    start_cost: entry.start_cost,
                    end_cost: entry.end_cost,
                    modified_objects: entry
                        .modified_objects
                        .iter()
                        .map(|id| id.to_vec())
                        .collect(),
                })
                .collect(),
        };

        let buf = log.encode_length_delimited_to_vec();
        if let Err(e) = self.current_file.write_all(&buf) {
            tracing::warn!("Failed to write congestion log: {e}");
            return;
        }
        self.current_file_size += buf.len() as u64;

        if self.current_file_size >= self.max_file_size
            && let Err(e) = self.rotate()
        {
            tracing::warn!("Failed to rotate congestion log: {e}");
        }
    }

    fn rotate(&mut self) -> io::Result<()> {
        let next_suffix = self.current_suffix + 1;
        let new_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(Self::file_path_for(&self.base_path, next_suffix))?;
        self.current_suffix = next_suffix;
        self.current_file = new_file;
        self.current_file_size = 0;
        self.delete_excess_files();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use prost::Message;
    use sui_types::base_types::ObjectID;
    use sui_types::digests::TransactionDigest;

    use super::super::shared_object_congestion_tracker::TransactionCostLogEntry;

    fn make_test_data() -> (ConsensusCommitInfo, FinishedCommitData) {
        let commit_info = ConsensusCommitInfo::new_for_congestion_test(
            1,
            1000,
            std::time::Duration::from_micros(1_000_000),
        );
        let data = FinishedCommitData {
            accumulated_debts: vec![],
            log_entries: vec![TransactionCostLogEntry {
                tx_digest: TransactionDigest::random(),
                start_cost: 0,
                end_cost: 100,
                modified_objects: vec![ObjectID::random()],
            }],
            final_object_execution_costs: HashMap::from([(ObjectID::random(), 100)]),
            commit_budget: 500_000,
        };
        (commit_info, data)
    }

    #[test]
    fn test_protobuf_round_trip() {
        let obj_id = ObjectID::random();
        let tx_digest = TransactionDigest::random();
        let modified_obj = ObjectID::random();

        let log = proto::CongestionCommitLog {
            epoch: 42,
            round: 100,
            timestamp_ms: 1234567890,
            commit_budget: 500_000,
            for_randomness: false,
            final_object_execution_costs: vec![proto::ObjectCost {
                object_id: obj_id.to_vec(),
                cost: 1000,
            }],
            transaction_entries: vec![proto::TransactionCostEntry {
                tx_digest: tx_digest.into_inner().to_vec(),
                start_cost: 50,
                end_cost: 150,
                modified_objects: vec![modified_obj.to_vec()],
            }],
        };

        let buf = log.encode_length_delimited_to_vec();
        let decoded = proto::CongestionCommitLog::decode_length_delimited(buf.as_slice()).unwrap();

        assert_eq!(decoded.epoch, 42);
        assert_eq!(decoded.round, 100);
        assert_eq!(decoded.timestamp_ms, 1234567890);
        assert_eq!(decoded.commit_budget, 500_000);
        assert!(!decoded.for_randomness);

        assert_eq!(decoded.final_object_execution_costs.len(), 1);
        let c = &decoded.final_object_execution_costs[0];
        assert_eq!(ObjectID::try_from(c.object_id.as_slice()).unwrap(), obj_id);
        assert_eq!(c.cost, 1000);

        assert_eq!(decoded.transaction_entries.len(), 1);
        let e = &decoded.transaction_entries[0];
        let digest_bytes: [u8; 32] = e.tx_digest.as_slice().try_into().unwrap();
        assert_eq!(TransactionDigest::new(digest_bytes), tx_digest);
        assert_eq!(e.start_cost, 50);
        assert_eq!(e.end_cost, 150);
        assert_eq!(
            e.modified_objects
                .iter()
                .map(|b| ObjectID::try_from(b.as_slice()).unwrap())
                .collect::<Vec<_>>(),
            vec![modified_obj]
        );
    }

    #[test]
    fn test_file_rotation() {
        let dir = tempfile::tempdir().unwrap();
        let base_path = dir.path().join("congestion_log");
        let config = CongestionLogConfig {
            path: base_path.clone(),
            max_file_size: 200,
            max_files: 3,
        };

        let mut logger = CongestionCommitLogger::new(&config).unwrap();
        let (commit_info, data) = make_test_data();
        for _ in 0..10 {
            logger.write_commit_log(1, &commit_info, false, &data);
        }

        let mut suffixes = CongestionCommitLogger::existing_suffixes(&base_path);
        suffixes.sort();
        assert!(suffixes.len() <= 3);
        assert_eq!(*suffixes.last().unwrap(), logger.current_suffix);
        assert!(suffixes.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn test_restart_resumes_after_highest_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let base_path = dir.path().join("congestion_log");
        let config = CongestionLogConfig {
            path: base_path.clone(),
            max_file_size: 200,
            max_files: 3,
        };
        let (commit_info, data) = make_test_data();

        let suffix_after_first_session;
        {
            let mut logger = CongestionCommitLogger::new(&config).unwrap();
            for _ in 0..10 {
                logger.write_commit_log(1, &commit_info, false, &data);
            }
            suffix_after_first_session = logger.current_suffix;
        }

        let logger2 = CongestionCommitLogger::new(&config).unwrap();
        assert_eq!(logger2.current_suffix, suffix_after_first_session + 1);
        assert_eq!(logger2.current_file_size, 0);
    }

    #[test]
    fn test_restart_always_starts_fresh_file() {
        let dir = tempfile::tempdir().unwrap();
        let base_path = dir.path().join("congestion_log");
        let config = CongestionLogConfig {
            path: base_path.clone(),
            max_file_size: 100_000,
            max_files: 3,
        };

        std::fs::write(
            CongestionCommitLogger::file_path_for(&base_path, 0),
            vec![0u8; 10],
        )
        .unwrap();

        let logger = CongestionCommitLogger::new(&config).unwrap();
        assert_eq!(logger.current_suffix, 1);
        assert_eq!(logger.current_file_size, 0);
    }

    #[test]
    fn test_old_files_deleted_on_rotation() {
        let dir = tempfile::tempdir().unwrap();
        let base_path = dir.path().join("congestion_log");
        let config = CongestionLogConfig {
            path: base_path.clone(),
            max_file_size: 200,
            max_files: 3,
        };

        let mut logger = CongestionCommitLogger::new(&config).unwrap();
        let (commit_info, data) = make_test_data();
        for _ in 0..20 {
            logger.write_commit_log(1, &commit_info, false, &data);
        }

        let suffixes = CongestionCommitLogger::existing_suffixes(&base_path);
        assert!(suffixes.len() <= 3);
        assert!(!CongestionCommitLogger::file_path_for(&base_path, 0).exists());
    }
}
