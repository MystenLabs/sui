// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// '#[error]' + 'public(package)' is rejected at the definition, even if the constant is never
// used

module 0x42::a {

#[error]
public(package) const ENotFound: vector<u8> = b"not found";

}
