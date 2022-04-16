// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum TraceTag {
    LatencyProbe,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Header {
    TraceId(u64),
    TraceTag(TraceTag),
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct SerializedHeaders {
    // Zero or more Header instances. Only the first
    // occurrence of a variant is read, subsequent ones
    // are discarded.
    pub headers: Vec<Header>,
}

#[derive(Default)]
pub struct Headers {
    trace_id: Option<u64>,
    trace_tag: Option<TraceTag>,
}

impl Headers {
    pub fn get_trace_id(&self) -> Option<&u64> {
        self.trace_id.as_ref()
    }

    pub fn get_trace_tag(&self) -> Option<&TraceTag> {
        self.trace_tag.as_ref()
    }
}

impl From<&SerializedHeaders> for Headers {
    fn from(serialized_headers: &SerializedHeaders) -> Self {
        let mut headers: Headers = Default::default();
        for header in serialized_headers.headers.iter() {
            match header {
                Header::TraceId(id) => {
                    if headers.trace_id.is_none() {
                        headers.trace_id = Some(*id);
                    }
                }
                Header::TraceTag(tag) => {
                    if headers.trace_tag.is_none() {
                        headers.trace_tag = Some(*tag);
                    }
                }
            }
        }
        headers
    }
}
