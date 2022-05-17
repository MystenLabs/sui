// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Exercise test functions that create, transfer, read, update, and delete objects

//# init --accounts A B

//# run Sui::ObjectBasics::create --sender A --args 10 @A

//# view-object 105

//# run Sui::ObjectBasics::transfer --sender A --args object(105) @B

//# view-object 105

//# run Sui::ObjectBasics::create --sender B --args 20 @B

//# run Sui::ObjectBasics::update --sender B --args object(105) object(108) --view-events

//# run Sui::ObjectBasics::delete --sender B --args object(105)
