// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::convert::Infallible;

use rxrust::subject::SubjectThreads;

pub mod evm_listener;
pub mod sui_listener;

pub type Subject<T> = SubjectThreads<T, Infallible>;
