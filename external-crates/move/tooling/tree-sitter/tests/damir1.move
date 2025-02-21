// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module 0x0::o {
  public fun test() {
    to_vec_set<u64>(vector[1,5,3,4])
  }
}
