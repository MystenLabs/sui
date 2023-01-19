// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Header from "./header/Header";
import Footer from "./footer/Footer";
import { ReactElement } from "react";

function Layout({ children }: { children: ReactElement | ReactElement[] }) {
  return (
    <>
      <Header />
      <div className="container">
        <div className="mx-auto max-w-4xl container">{children}</div>
        <Footer />
      </div>
    </>
  );
}

export default Layout;
