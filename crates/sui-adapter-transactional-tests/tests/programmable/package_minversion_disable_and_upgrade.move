// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Permanent disable and an ordinary upgrade can occur in one PTB. The cap remains outside
// minversion, so the normal authorize/Upgrade/commit sequence needs no PackageConfig input.

//# init --addresses BaseV1=0x0 BaseV2=0x0 --accounts A

//# publish --upgradeable --sender A
module BaseV1::base;
public fun ping() { abort 1 }

//# stage-package
module BaseV2::base;
public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 2 }) }

//# programmable --sender A --inputs object(1,1) 0u8 digest(BaseV2)
//> 0: sui::package::disable_minversion_permanently(Input(0));
//> 1: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 2: Upgrade(BaseV2, [sui,std], BaseV1, Result(1));
//> sui::package::commit_upgrade(Input(0), Result(2));

// The cap now identifies the upgraded package and its second version.
//# run sui::package::upgrade_package --args object(1,1) --sender A

//# run sui::package::version --args object(1,1) --sender A
