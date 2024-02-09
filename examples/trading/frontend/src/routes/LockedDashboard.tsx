// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from "react";
import { Tabs } from "@radix-ui/themes";
import { LockItems } from "../components/locked/LockItems";
import { LockedList } from "../components/locked/LockedList";
import { useCurrentAccount } from "@mysten/dapp-kit";

// SPDX-License-Identifier: Apache-2.0
export function LockedDashboard() {
  const account = useCurrentAccount();
  const tabs = [
    {
      name: "My Locked Objects",
      component: () => (
        <LockedList
          params={{
            deleted: "false",
            creator: account?.address,
          }}
        />
      ),
    },
    {
      name: "Lock Owned objects",
      component: () => <LockItems />,
    },
  ];

  const [tab, setTab] = useState(tabs[0].name);

  return (
    <Tabs.Root value={tab} onValueChange={setTab}>
      <Tabs.List>
        {tabs.map((tab, index) => {
          return (
            <Tabs.Trigger
              key={index}
              value={tab.name}
              className="cursor-pointer"
            >
              {tab.name}
            </Tabs.Trigger>
          );
        })}
      </Tabs.List>
      {tabs.map((tab, index) => {
        return (
          <Tabs.Content key={index} value={tab.name}>
            {tab.component()}
          </Tabs.Content>
        );
      })}
    </Tabs.Root>
  );
}
