// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from "vitest";
import { BCS, getSuiMoveConfig } from "../src/index";

describe("de/ser of inline struct definitions", () => {
  it("should de/serialize inline definition", () => {
    const bcs = new BCS(getSuiMoveConfig());
    const value = {
      t1: "Adam",
      t2: 1000n,
      t3: ["aabbcc", "00aa00", "00aaffcc"],
    };

    expect(
      serde(
        bcs,
        {
          t1: "string",
          t2: "u64",
          t3: "vector<hex-string>",
        },
        value
      )
    ).toEqual(value);
  });

  it("should not contain a trace of the temp struct", () => {
    const bcs = new BCS(getSuiMoveConfig());
    const _sr = bcs
      .ser({ name: "string", age: "u8" }, { name: "Charlie", age: 10 })
      .toString("hex");

    expect(bcs.hasType("temp_struct")).toBe(false);
  });
});

function serde(bcs, type, data) {
  let ser = bcs.ser(type, data).toString("hex");
  let de = bcs.de(type, ser, "hex");
  return de;
}
