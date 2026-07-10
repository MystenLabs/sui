// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost_reflect::DescriptorPool;
use tower::Layer;

use super::GrpcRequestLog;

/// Large enough for any Sui transaction payload (the protocol caps transactions well below this),
/// and deliberately smaller than tonic's 4 MiB decode limit: the captured payload is re-emitted as
/// a single JSON log line, so this also bounds log-line size and the memory buffered per request.
/// Larger frames are skipped rather than buffered.
const DEFAULT_MAX_CAPTURED_MESSAGE_SIZE: usize = 512 * 1024;

/// Layer that applies [`GrpcRequestLog`] to a service.
///
/// See the [module docs](super) for how captures are enabled and collected.
#[derive(Clone)]
pub struct GrpcRequestLogLayer {
    pool: DescriptorPool,
    max_captured_message_size: usize,
}

impl GrpcRequestLogLayer {
    pub fn new(pool: DescriptorPool) -> Self {
        Self {
            pool,
            max_captured_message_size: DEFAULT_MAX_CAPTURED_MESSAGE_SIZE,
        }
    }

    /// Build the descriptor pool from the same encoded `FileDescriptorSet`s the server registers
    /// for gRPC reflection. Sets are decoded one at a time: files already added by an earlier set
    /// are skipped (so well-known types appearing in several sets built with `--include_imports`
    /// are fine), while duplicates *within* one set are malformed and error. A set may reference
    /// files from the sets before it, so dependencies must come first.
    pub fn from_encoded_file_descriptor_sets<'a>(
        sets: impl IntoIterator<Item = &'a [u8]>,
    ) -> Result<Self, prost_reflect::DescriptorError> {
        let mut pool = DescriptorPool::new();
        for set in sets {
            pool.decode_file_descriptor_set(set)?;
        }
        Ok(Self::new(pool))
    }

    pub fn with_max_captured_message_size(mut self, bytes: usize) -> Self {
        self.max_captured_message_size = bytes;
        self
    }
}

impl<S> Layer<S> for GrpcRequestLogLayer {
    type Service = GrpcRequestLog<S>;

    fn layer(&self, inner: S) -> Self::Service {
        GrpcRequestLog::new(inner, self.pool.clone(), self.max_captured_message_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same file appearing in more than one set must be skipped, not error — servers register
    /// well-known types alongside sets built with `--include_imports` that embed them again.
    #[test]
    fn duplicate_files_across_sets_are_skipped() {
        GrpcRequestLogLayer::from_encoded_file_descriptor_sets([
            tonic_health::pb::FILE_DESCRIPTOR_SET,
            tonic_health::pb::FILE_DESCRIPTOR_SET,
        ])
        .unwrap();
    }
}
