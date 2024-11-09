// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context::Context;

pub(crate) fn certificate_server_name(context: &Context) -> String {
    format!("consensus_epoch_{}", context.committee.epoch())
}
