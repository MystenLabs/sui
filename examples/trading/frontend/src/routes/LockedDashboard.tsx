// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from "react";
import { Tabs } from "@radix-ui/themes";
import { LockOwnedObjects } from "@/components/locked/LockOwnedObjects";
import { OwnedLockedList } from "@/components/locked/OwnedLockedList";

export function LockedDashboard() {
  const tabs = [
    {
      name: "My Locked Objects",
      component: () => <OwnedLockedList />,
    },
    {
      name: "Lock Owned objects",
      component: () => <LockOwnedObjects />,
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
