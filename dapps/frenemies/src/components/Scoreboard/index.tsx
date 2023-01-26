// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Tab } from "@headlessui/react";
import { ReactNode, useState } from "react";
import { Card } from "../Card";
import { Leaderboard } from "../leaderboard/Leaderboard";
import { YourScore } from "../your-score/YourScore";

function TabItem({ children }: { children: ReactNode }) {
  return (
    <Tab className="font-semibold leading-tight px-5 py-2 rounded-full ui-selected:bg-white ui-selected:text-steel-darker ui-not-selected:bg-transparent ui-not-selected:text-white">
      {children}
    </Tab>
  );
}

export function Scoreboard() {
  const [selectedIndex, setSelectedIndex] = useState(0);

  return (
    <Card variant={selectedIndex === 0 ? "score" : "leaderboard"}>
      <div className="relative">
        <Tab.Group selectedIndex={selectedIndex} onChange={setSelectedIndex}>
          <Tab.List className="inline-flex space-x-1 rounded-full bg-white/[15%]">
            <TabItem>You</TabItem>
            <TabItem>Leaderboard</TabItem>
          </Tab.List>
          <Tab.Panels>
            <Tab.Panel>
              <YourScore />
            </Tab.Panel>
            <Tab.Panel>
              <Leaderboard />
            </Tab.Panel>
          </Tab.Panels>
        </Tab.Group>
      </div>
    </Card>
  );
}
