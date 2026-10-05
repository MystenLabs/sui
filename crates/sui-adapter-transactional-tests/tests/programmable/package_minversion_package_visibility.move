// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `public(package)` replaces the deprecated `friend` mechanism for package-internal APIs. The
// Consumer's entry call pins its historical PackageV1 dependency exact; minversion selects
// PackageV2 without making the package-internal API externally callable.

//# init --addresses PackageV1=0x0 PackageV2=0x0 Consumer=0x0 --accounts A

//# publish --upgradeable --sender A
module PackageV1::api {
    public(package) fun value(): u64 { 1 }
}

module PackageV1::entry {
    use PackageV1::api;

    public fun value(): u64 { api::value() }
}

//# publish --dependencies PackageV1 --sender A
module Consumer::consumer;
use PackageV1::entry;

public struct Result has key, store { id: sui::object::UID, value: u64 }

public fun new(ctx: &mut sui::tx_context::TxContext) {
    sui::transfer::share_object(Result { id: sui::object::new(ctx), value: 0 })
}

entry fun call(result: &mut Result) { result.value = entry::value() }

//# run Consumer::consumer::new --sender A

//# run Consumer::consumer::call --args object(3,0) --sender A

//# view-object 3,0

//# upgrade --package PackageV1 --upgrade-capability 1,1 --sender A
module PackageV2::api {
    public(package) fun value(): u64 { 2 }
}

module PackageV2::entry {
    use PackageV2::api;

    public fun value(): u64 { api::value() }
}

// The Consumer retains its exact historical PackageV1 reference before minversion enrollment.
//# run Consumer::consumer::call --args object(3,0) --sender A

//# view-object 3,0

//# programmable --sender A --inputs object(1,1) object(0x426)
//> 0: sui::package::enable_minversion(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// The exact historical reference immediately selects PackageV2, whose public(package) API is callable
// only by the sibling entry module in the selected package.
//# run Consumer::consumer::call --args object(3,0) --sender A

//# view-object 3,0
