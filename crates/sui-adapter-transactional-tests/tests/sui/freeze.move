// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// test that freezing prevents transfers/mutations

//# init --accounts A

//# run sui::ObjectBasics::create --args 10 @A

//# run sui::ObjectBasics::freeze_object --args object(104)

//# run sui::ObjectBasics::transfer --args object(104) @A

//# run sui::ObjectBasics::set_value --args object(104) 1
