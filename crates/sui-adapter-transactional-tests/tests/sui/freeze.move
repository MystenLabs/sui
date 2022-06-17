// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// test that freezing prevents transfers/mutations

//# init --accounts A

//# run sui::object_basics::create --args 10 @A

//# run sui::object_basics::freeze_object --args object(104)

//# run sui::object_basics::transfer --args object(104) @A

//# run sui::object_basics::set_value --args object(104) 1
