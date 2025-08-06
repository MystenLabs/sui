// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use move_trace_format::format::{MoveTrace, MoveTraceReader};
use sui_types::{effects::TransactionEffects, gas::GasUsageReport};

pub const ARTIFACTS_ENCODING_EXT: &str = "json";
pub const ARTIFACTS_ENCODING_COMPRESSION_EXT: &str = "json.zst";

pub const ARTIFACTS: [Artifact; 4] = [
    Artifact::Trace,
    Artifact::TransactionEffects,
    Artifact::TransactionGasReport,
    Artifact::ForkedTransactionEffects,
];

/// The types of artifacts that the replay tool knows about and may output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Artifact {
    Trace,
    TransactionEffects,
    TransactionGasReport,
    ForkedTransactionEffects,
}

/// Encoding types for artifacts that may be output by the replay tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodingType {
    Json,
    JsonCompressed,
}

/// Manages artifacts produced by the replay tool. An `ArtifactManager` is always with respect to a
/// given base path (e.g., the output replay directory for a specific transaction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactManager<'a> {
    pub base_path: &'a Path,
    pub overrides_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactMember<'a, 'b> {
    manager: &'a ArtifactManager<'b>,
    artifact_type: Artifact,
    artifact_path: PathBuf,
}

impl EncodingType {
    /// Returns the file extension associated with the encoding type.
    pub const fn ext(&self) -> &str {
        match self {
            EncodingType::Json => ARTIFACTS_ENCODING_EXT,
            EncodingType::JsonCompressed => ARTIFACTS_ENCODING_COMPRESSION_EXT,
        }
    }
}

impl<'b> ArtifactManager<'b> {
    /// Creates a new `ArtifactManager` with the given base path and whether overrides are allowed.
    pub fn new(base_path: &'b Path, overrides_allowed: bool) -> anyhow::Result<Self> {
        std::fs::create_dir_all(base_path).map_err(|e| {
            anyhow::anyhow!(
                "Failed to create base path for replay artifacts at {}: {e}",
                base_path.display()
            )
        })?;
        Ok(ArtifactManager {
            base_path,
            overrides_allowed,
        })
    }

    /// Create an `ArtifactManager` for this artifact type, rooted at the given `base_path`.
    pub fn member<'a>(&'a self, artifact: Artifact) -> ArtifactMember<'a, 'b> {
        ArtifactMember {
            artifact_path: self.base_path.join(artifact.as_file()),
            manager: self,
            artifact_type: artifact,
        }
    }
}

impl Artifact {
    /// Returns the string representation of the artifact type.
    pub const fn as_str(&self) -> &str {
        match self {
            Artifact::Trace => "trace",
            Artifact::TransactionEffects => "transaction_effects",
            Artifact::ForkedTransactionEffects => "forked_transaction_effects",
            Artifact::TransactionGasReport => "transaction_gas_report",
        }
    }

    /// Encoding type for each artifact.
    pub fn encoding_type(&self) -> EncodingType {
        match self {
            Artifact::Trace => EncodingType::JsonCompressed,
            Artifact::ForkedTransactionEffects
            | Artifact::TransactionEffects
            | Artifact::TransactionGasReport => EncodingType::Json,
        }
    }

    /// Returns the file for the artifact, including its encoding type. The returned `PathBuf` is
    /// not rooted.
    pub fn as_file(&self) -> PathBuf {
        PathBuf::from(format!("{}.{}", self.as_str(), self.encoding_type().ext()))
    }
}

/// Deserialization methods for `ArtifactManager`.
impl ArtifactMember<'_, '_> {
    pub fn exists(&self) -> bool {
        self.artifact_path.exists()
    }

    /// Deserialize the artifact into json. This should always succeed if the artifact exists and
    /// is well-formed.
    pub fn get_json(&self) -> anyhow::Result<serde_json::Value> {
        let file_content = std::fs::read(&self.artifact_path)?;
        let contents = match self.artifact_type.encoding_type() {
            EncodingType::Json => file_content,
            EncodingType::JsonCompressed => {
                let mut buf = Vec::new();
                zstd::bulk::decompress_to_buffer(&file_content, &mut buf)?;
                buf
            }
        };
        Ok(serde_json::from_slice(&contents)?)
    }

    /// Try to get the trace reader if the artifact type is a trace.
    /// If the artifact type is not `Trace` `None` is returned.
    pub fn try_get_trace(&self) -> Option<anyhow::Result<MoveTraceReader<'_, std::fs::File>>> {
        if self.artifact_type == Artifact::Trace {
            Some(
                std::fs::File::open(&self.artifact_path)
                    .and_then(MoveTraceReader::new)
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "Failed to open trace file {}: {e}",
                            self.artifact_path.display()
                        )
                    }),
            )
        } else {
            None
        }
    }

    /// Try to get the transaction effects if the artifact type is `TransactionEffects`.
    /// If the artifact type is not `TransactionEffects` `None` is returned.
    pub fn try_get_transaction_effects(&self) -> Option<anyhow::Result<TransactionEffects>> {
        if matches!(
            self.artifact_type,
            Artifact::TransactionEffects | Artifact::ForkedTransactionEffects
        ) {
            Some(self.get_json().and_then(|json| {
                serde_json::from_value::<TransactionEffects>(json).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to deserialize transaction effects from {}: {e}",
                        self.artifact_path.display()
                    )
                })
            }))
        } else {
            None
        }
    }

    /// Try to get the GasUsageReport if the artifact type is `TransactionGasReport`.
    /// If the artifact type is not `TransactionGasReport` `None` is returned.
    pub fn try_get_gas_report(&self) -> Option<anyhow::Result<GasUsageReport>> {
        if self.artifact_type == Artifact::TransactionGasReport {
            Some(self.get_json().and_then(|json| {
                serde_json::from_value::<GasUsageReport>(json).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to deserialize gas usage report from {}: {e}",
                        self.artifact_path.display()
                    )
                })
            }))
        } else {
            None
        }
    }
}

/// Serialization methods for `ArtifactManager`.
impl ArtifactMember<'_, '_> {
    pub fn serialize_move_trace(&self, trace: MoveTrace) -> Option<anyhow::Result<()>> {
        if self.artifact_type != Artifact::Trace {
            return None;
        }

        if !self.manager.overrides_allowed && self.artifact_path.exists() {
            return Some(Err(anyhow::anyhow!(
                "Trace file already exists at {}",
                self.artifact_path.display()
            )));
        }

        Some(
            std::fs::write(&self.artifact_path, trace.into_compressed_json_bytes()).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to write trace to {}: {e}",
                    self.artifact_path.display()
                )
            }),
        )
    }

    pub fn serialize_artifact(&self, data: &impl serde::Serialize) -> Option<anyhow::Result<()>> {
        if self.artifact_type == Artifact::Trace {
            return None;
        }

        if !self.manager.overrides_allowed && self.artifact_path.exists() {
            return Some(Err(anyhow::anyhow!(
                "File for {} already exists at {}",
                self.artifact_type.as_str(),
                self.artifact_path.display()
            )));
        }
        let mut file = match std::fs::File::create(&self.artifact_path).map_err(|e| {
            anyhow::anyhow!(
                "Failed to create file {}: {e}",
                self.artifact_path.display()
            )
        }) {
            Ok(file) => file,
            Err(e) => return Some(Err(e)),
        };

        Some(match self.artifact_type.encoding_type() {
            EncodingType::Json => serde_json::to_writer(file, &data)
                .map_err(|e| anyhow::anyhow!("Failed to write JSON: {e}")),
            EncodingType::JsonCompressed => {
                let mut buf = Vec::new();
                let mut compressed_buf = Vec::new();
                if let Err(e) = serde_json::to_writer(&mut buf, &data)
                    .map_err(|e| anyhow::anyhow!("Failed to write JSON: {e}"))
                {
                    return Some(Err(e));
                }
                zstd::bulk::compress_to_buffer(&buf, &mut compressed_buf, 0)
                    .map_err(|e| anyhow::anyhow!("Failed to compress JSON: {e}"))
                    .and_then(|_| {
                        file.write_all(&buf).map_err(|e| {
                            anyhow::anyhow!(
                                "Failed to write compressed JSON to {}: {e}",
                                self.artifact_path.display()
                            )
                        })
                    })
            }
        })
    }
}
