// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // let host = "http://127.0.0.1";
    // let server_port = 9001;

    // let _ = startup::start_server(None, host, server_port, None, None).await?;

    Ok(())
}
