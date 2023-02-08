// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import "@fontsource/inter/variable.css";
import "./index.css";
import Plausible from "plausible-tracker";
import React from "react";
import ReactDOM from "react-dom/client";
import { createBrowserRouter, RouterProvider } from "react-router-dom";
import { QueryClientProvider, QueryClient } from "@tanstack/react-query";

import { Root } from "./routes/Root";
import { Home } from "./routes/Home";
import { Connect } from "./routes/Connect";
import { Setup } from "./routes/Setup";
import { toast } from "react-hot-toast";

const plausible = Plausible({});
plausible.enableAutoPageviews();

const queryClient = new QueryClient({
  defaultOptions: {
    mutations: {
      onError(error) {
        toast.error(String(error));
      },
    },
  },
});

const router = createBrowserRouter([
  {
    path: "/",
    element: <Root />,
    children: [
      {
        path: "",
        element: <Home />,
      },
      {
        path: "connect",
        element: <Connect />,
      },
      {
        path: "setup",
        element: <Setup />,
      },
    ],
  },
]);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </React.StrictMode>
);
