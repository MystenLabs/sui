// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0


//# init --addresses test=0x0 --accounts A

//# programmable --sender A 
//> 0: MakeMoveVec<std::string::utf8>([]);

//# programmable --sender A --inputs 1
//> 0: MakeMoveVec<std::string::utf8>([Input(0)]);
