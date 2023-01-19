// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Card } from "../Card";
import { Balance } from "./Balance";
import { Table } from "./Table";

export function Validators() {
  return (
    <Card variant="white" spacing="lg">
      <div className="flex items-center justify-between mb-10">
        <h2 className="text-steel-dark font-normal text-2xl">
          Stake SUI to achieve your goal as a{" "}
          <span className="font-bold">friend</span>.
        </h2>

        <Balance />
      </div>
      <Table />
    </Card>
  );
}
