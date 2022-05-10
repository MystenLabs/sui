// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Exercise test functions that wrap and object and subsequently unwrap it
// Ensure that the object's version is consistent

//# init --accounts A

//# run Sui::ObjectBasics::create --args 10 @A

//# view-object 104

//# run Sui::ObjectBasics::wrap --args object(104) --sender A

//# run Sui::ObjectBasics::unwrap --args object(106) --sender A

//# view-object 104
