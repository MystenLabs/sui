// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[path = "generated/sui.rest.rs"]
mod generated;
pub use generated::*;
use tap::Pipe;

impl TryFrom<Vec<crate::checkpoints::CheckpointResponse>> for CheckpointPage {
    type Error = bcs::Error;
    fn try_from(value: Vec<crate::checkpoints::CheckpointResponse>) -> Result<Self, Self::Error> {
        let checkpoints = value
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(Self { checkpoints })
    }
}

impl TryFrom<crate::checkpoints::CheckpointResponse> for Checkpoint {
    type Error = bcs::Error;

    fn try_from(c: crate::checkpoints::CheckpointResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            summary: Some(bcs::to_bytes(&c.summary)?.into()),
            signature: Some(bcs::to_bytes(&c.signature)?.into()),
            contents: c
                .contents
                .as_ref()
                .map(bcs::to_bytes)
                .transpose()?
                .map(Into::into),
        })
    }
}

impl TryFrom<Checkpoint> for crate::checkpoints::CheckpointResponse {
    type Error = bcs::Error;

    fn try_from(value: Checkpoint) -> Result<Self, Self::Error> {
        let summary = value
            .summary
            .ok_or_else(|| bcs::Error::Custom("missing summary".into()))?
            .pipe(|bytes| bcs::from_bytes(&bytes))?;
        let signature = value
            .signature
            .ok_or_else(|| bcs::Error::Custom("missing signature".into()))?
            .pipe(|bytes| bcs::from_bytes(&bytes))?;

        let contents = value
            .contents
            .map(|bytes| bcs::from_bytes(&bytes))
            .transpose()?;

        Ok(Self {
            summary,
            signature,
            contents,
        })
    }
}
