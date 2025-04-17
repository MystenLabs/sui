// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Boogie model for vectors, based on smt arrays.

// This version of vectors requires boogie to be called without `-useArrayAxiom`.
// It is not extensional.

// Currently we just include the basic vector array theory.

{% include "vector-array-theory" %}
