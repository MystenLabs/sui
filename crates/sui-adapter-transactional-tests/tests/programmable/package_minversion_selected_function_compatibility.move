// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Package upgrades may remove a private entry function or change its parameters. After this package
// family selects v2, a transaction that names the old v1 package ID is redirected to v2 before the
// function is looked up. The call must therefore fail if v2 removed the function or changed its
// signature, and succeed only when its arguments match the v2 signature.

//# init --addresses FunctionsV1=0x0 FunctionsV2=0x0 --accounts A

//# publish --upgradeable --sender A
module FunctionsV1::functions;
public struct Ping has copy, drop { value: u64 }
entry fun removed() { sui::event::emit(Ping { value: 1 }) }
entry fun changed(_value: u64) { sui::event::emit(Ping { value: 1 }) }

//# upgrade --package FunctionsV1 --upgrade-capability 1,1 --sender A
module FunctionsV2::functions;
public struct Ping has copy, drop { value: u64 }
entry fun changed(_value: bool) { sui::event::emit(Ping { value: 2 }) }

//# programmable --sender A --inputs object(1,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// Root selection happens before function lookup, so the removed v1 entry is not found.
//# run FunctionsV1::functions::removed --sender A

// Arguments are checked against the selected v2 signature, not the historical v1 signature.
//# run FunctionsV1::functions::changed --args 7 --sender A

// A historical package reference with the selected signature executes v2.
//# run FunctionsV1::functions::changed --args true --sender A
