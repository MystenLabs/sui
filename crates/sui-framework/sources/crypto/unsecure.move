// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::unsecure {
    friend sui::validator;

    public native fun unsecure_verify(signature: &vector<u8>, public_key: &vector<u8>, msg: &vector<u8>): bool;

    public(friend) fun unsecure_verify_with_domain(
        signature: &vector<u8>,
        public_key: &vector<u8>,
        msg: vector<u8>,
        domain: vector<u8>
    ): bool {
        std::vector::append(&mut domain, msg);
        unsecure_verify(signature, public_key, &domain)
    }

}
