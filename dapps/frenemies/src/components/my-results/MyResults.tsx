// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Row, Record } from "./Row";
import { TableHeader } from "./TableHeader";

/**
 * Table representing game score for the user.
 */
export function MyResults({ records }: { records: Record[] }) {
  return (
    <div className="w-auto">
      <table className="table-fixed w-auto">
        <TableHeader />
        {records.map((record) => (
          <Row record={record} />
        ))}
      </table>
    </div>
  );
}
