// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::prover {

    spec native fun owned(memory: address, id: address): bool;

    spec native fun owned_by(memory: address, id: address, owner: address): bool;

    spec native fun shared(memory: address, id: address): bool;

}
