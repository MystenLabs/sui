// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from "clsx";
import { ReactNode } from "react";

interface StatProps {
  variant?: "leaderboard" | "default";
  label: string;
  children: ReactNode;
}

export function Stat({ label, children, variant = "default" }: StatProps) {
  return (
    <div className="flex flex-col gap-1">
      <div
        className={clsx(
          "text-base leading-tight uppercase",
          variant === "leaderboard" ? "text-white" : "text-steel-dark"
        )}
      >
        {label}
      </div>
      <div
        className={clsx(
          "text-3xl font-semibold",
          variant === "leaderboard" ? "text-white" : "text-frenemies"
        )}
      >
        {children}
      </div>
    </div>
  );
}
