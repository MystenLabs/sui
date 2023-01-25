// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Header } from "./Header";
import { Footer } from "./Footer";
import { ReactElement } from "react";

export function Layout({ children }: { children: ReactElement | ReactElement[] }) {
  return (
    <>
      <Header />
      <div className="mx-auto max-w-6xl px-4 w-full flex flex-col gap-5 mb-24 mt-4">
        {children}
      </div>
      <Footer />
    </>
  );
}
