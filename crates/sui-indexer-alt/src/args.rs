// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use url::Url;

#[derive(clap::Parser, Debug, Clone)]
pub struct Args {
    #[arg(long)]
    pub remote_store_url: Url,
}
