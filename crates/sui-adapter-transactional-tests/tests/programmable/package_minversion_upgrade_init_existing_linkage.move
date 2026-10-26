// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A call before an upgrade places Root's family in the transaction-wide linkage table. The same
// PTB then upgrades Root and introduces a new module with init. The enrolled dependency family must
// resolve consistently to DepV2 for both the earlier call and the new initializer.

//# init --addresses DepV1=0x0 DepV2=0x0 RootV1=0x0 RootV2=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::dep;
public fun value(): u64 { 1 }

//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::dep;
public fun value(): u64 { 2 }

//# publish --upgradeable --dependencies DepV1 --sender A
module RootV1::root;
use DepV1::dep;
public struct Call has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Call { version: dep::value() }) }

//# stage-package --dependencies DepV2
module RootV2::root {
    use DepV1::dep;
    public struct Call has copy, drop { version: u64 }
    public fun ping() { sui::event::emit(Call { version: dep::value() }) }
}

module RootV2::init_module {
    use DepV1::dep;
    public struct Config has key { id: sui::object::UID, version: u64 }
    fun init(ctx: &mut sui::tx_context::TxContext) {
        sui::transfer::share_object(Config { id: sui::object::new(ctx), version: dep::value() })
    }
}

//# programmable --sender A --inputs object(1,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// Command 0 inserts RootV1 and selected DepV2 into unified linkage before the upgrade-with-init is
// analyzed. The call event and shared Config both record dependency version 2.
//# programmable --sender A --inputs object(3,1) 0u8 digest(RootV2)
//> 0: RootV1::root::ping();
//> 1: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 2: Upgrade(RootV2, [DepV2,sui,std], RootV1, Result(1));
//> sui::package::commit_upgrade(Input(0), Result(2));

//# view-object 6,1
