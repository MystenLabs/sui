// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module a::m;

use std::string::String;
use std::ascii::String as AsciiString;

public fun t1() {
    let s: String = "hello";
    let s: AsciiString = "hello";
    let s: String = "hello\"";
    let s: String = "hello\a\t";
    let s: String = "hello\n";
    let s: String = "hello\r";
    let s: String = "hello\t";
    let s: String = "hello\0";
    let s: String = "hello\xA0";
}
