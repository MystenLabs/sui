// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { createContext, useContext } from "react";
import type { JSX } from "react";
import type { ReactNode } from "react";

type StepContextType = {
  prefix: string;
  depth: number;
  headingLevel: number;
};

const StepContext = createContext<StepContextType | null>(null);

export default function Steps({
  children,
  headingLevel = 2,
}: {
  children: ReactNode;
  headingLevel?: number;
}) {
  stepCounters = [];
  return (
    <StepContext.Provider value={{ prefix: "", depth: 0, headingLevel }}>
      {children}
    </StepContext.Provider>
  );
}

let stepCounters: number[] = [];

function Heading({ level, children }: { level: number; children: ReactNode }) {
  const Tag = `h${level}` as keyof JSX.IntrinsicElements;
  return <Tag>{children}</Tag>;
}

export function Step({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  const ctx = useContext(StepContext);
  if (!ctx) throw new Error("Step must be used within <Steps>");

  const depth = ctx.depth;
  stepCounters[depth] = (stepCounters[depth] || 0) + 1;
  stepCounters = stepCounters.slice(0, depth + 1);

  const number = stepCounters.slice(0, depth + 1).join(".");
  const prefix = number;
  const headingLevel = ctx.headingLevel + depth;

  return (
    <>
      <Heading level={headingLevel}>{`Step ${prefix}: ${title}`}</Heading>
      <StepContext.Provider
        value={{ prefix, depth: depth + 1, headingLevel: ctx.headingLevel }}
      >
        {children}
      </StepContext.Provider>
    </>
  );
}

export function SubStep(props: Parameters<typeof Step>[0]) {
  return <Step {...props} />;
}
