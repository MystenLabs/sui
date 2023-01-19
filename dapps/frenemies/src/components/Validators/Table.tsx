// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from "clsx";
import { ReactNode } from "react";
import { Stake } from "./Stake";

function Header({ children }: { children: ReactNode }) {
  return (
    <div className="text-left font-normal uppercase text-base text-steel-dark">
      {children}
    </div>
  );
}

function GridItem({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={clsx("grid", className)}
      style={{
        gridTemplateColumns:
          "minmax(100px, 1fr) minmax(100px, 2fr) minmax(150px, 5fr)",
      }}
    >
      {children}
    </div>
  );
}

export function Table() {
  return (
    <>
      <GridItem className="px-5 py-4">
        <Header>Rank</Header>
        <Header>Validator</Header>
        <Header>Your Sui Stake</Header>
      </GridItem>
      <GridItem className="px-5 py-2 rounded-xl bg-[#F5FAFA] text-steel-dark items-center">
        <div>1</div>
        <div>0xABCD...EFGH</div>
        <div>
          <Stake />
        </div>
      </GridItem>
    </>
  );
}
