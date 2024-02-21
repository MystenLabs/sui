// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createBrowserRouter, Navigate } from "react-router-dom";

import { Root } from "./root";
import { LockedDashboard } from "@/routes/LockedDashboard";
import { EscrowDashboard } from "@/routes/EscrowDashboard";

export const router = createBrowserRouter([
  {
    path: "/",
    element: <Root />,
    children: [
      {
        path: "/",
        element: <Navigate to="escrows" replace />,
      },
      {
        path: "escrows",
        element: <EscrowDashboard />,
      },
      {
        path: "locked",
        element: <LockedDashboard />,
      },
    ],
  },
]);
