// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A no-init publication must verify against its declared dependency v2 independently
// of runtime calls pinned to v1. Its payload calls a v2-only API, while the ordinary
// call checks that v1 actually executes. Tests both command orders and rejects the
// same payload when publication instead declares v1, even after v2 has been cached.

//# init --addresses DepV1=0x0 DepV2=0x0 NoInit=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::m;
public fun check(expected: u64) { assert!(expected == 1, 11) }

//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::m;
public fun check(expected: u64) { assert!(expected == 2, 22) }
public fun only_v2() {}

//# set-address DepV2 @DepV1
// Compile the v2 source at its original address so the payload uses original-ID module handles.

//# set-address DepV1 @0x0
// Hide the v1 source at a different address while compiling against the v2-only API.

//# stage-package
module NoInit::publication;
public fun use_new_api() { DepV2::m::only_v2() }

//# set-address DepV1 object(1,0)

//# set-address DepV2 object(2,0)
// Restore version-ID bindings before constructing transactions.

//# programmable --sender A --inputs 1u64 @A
// Verification needs v2, but the later ordinary call must still execute v1.
//> 0: Publish(NoInit, [DepV2,sui,std]);
//> DepV1::m::check(Input(0));
//> TransferObjects([Result(0)], Input(1));

//# programmable --sender A --inputs 1u64 @A
// The same isolation applies when the runtime call precedes publication.
//> DepV1::m::check(Input(0));
//> 1: Publish(NoInit, [DepV2,sui,std]);
//> TransferObjects([Result(1)], Input(1));

//# programmable --sender A --inputs 1u64 @A
// Declaring v1 must fail verification: only_v2 is absent, regardless of warmed v2 caches.
//> 0: Publish(NoInit, [DepV1,sui,std]);
//> DepV1::m::check(Input(0));
//> TransferObjects([Result(0)], Input(1));
