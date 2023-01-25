// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/** @type {import('tailwindcss').Config} */
module.exports = {
  presets: [require("@mysten/core/tailwind.config")],
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      boxShadow: {
        notification: "0px 0px 20px rgba(29, 55, 87, 0.11)",
        button: "0px 1px 2px rgba(16, 24, 40, 0.05)",
      },
      colors: {
        frenemies: "#768AF7",
      },
    },
  },
  plugins: [],
};
