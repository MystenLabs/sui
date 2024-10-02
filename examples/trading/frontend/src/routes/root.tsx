// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Toaster } from "react-hot-toast";
import { Outlet } from "react-router-dom";
import { Header } from "@/components/Header";
import { Container } from "@radix-ui/themes";

export function Root() {
  return (
    <div>
      <Toaster position="bottom-center" />
      <Header />
      <Container py="8">
        <Outlet />
      </Container>
    </div>
  );
}
