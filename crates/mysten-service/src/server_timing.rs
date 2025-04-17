// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use mysten_metrics::{add_server_timing, get_server_timing, with_new_server_timing};
use simple_server_timing_header::Timer;

pub async fn server_timing_middleware(request: Request, next: Next) -> Response {
    with_new_server_timing(async move {
        let mut response = next.run(request).await;
        add_server_timing("finish_request");

        if let Ok(header_value) = get_server_timing()
            .expect("server timing is not set")
            .lock()
            .header_value()
            .try_into()
        {
            response
                .headers_mut()
                .insert(Timer::header_key(), header_value);
        }
        response
    })
    .await
}
