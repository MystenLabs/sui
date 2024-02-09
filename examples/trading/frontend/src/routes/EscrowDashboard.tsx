// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from "react";
import { Tabs, Tooltip } from "@radix-ui/themes";
import { LockedList } from "../components/locked/LockedList";
import { EscrowList } from "../components/escrows/EscrowList";
import { InfoCircledIcon } from "@radix-ui/react-icons";

export function EscrowDashboard() {
  const tabs = [
    {
      name: "Requested Escrows",
      component: () => <EscrowList received />,
      tooltip: "Escrows requested for your locked objects.",
    },
    {
      name: "My Pending Requests",
      component: () => <EscrowList sent />,
      tooltip: "Escrows you have initiated for third party locked objects.",
    },
    {
      name: "Browse Locked Objects",
      component: () => <LockedList />,
      tooltip: "Browse locked objects you can trade for.",
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
              <Tooltip content={tab.tooltip}>
                <InfoCircledIcon className="ml-3" />
              </Tooltip>
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
