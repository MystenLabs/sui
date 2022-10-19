// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const defaultTheme = require("tailwindcss/defaultTheme");

module.exports = {
  content: ["./src/**/*.{js,jsx,ts,tsx}"],
  theme: {
    extend: {
      // Line-heights that are found in the design:
      lineHeight: {
        80: "0.8",
        100: "1",
        130: "1.3",
      },
      fontSize: {
        // Text sizes:
        body: ["14px", "1"],
        bodySmall: ["13px", "1"],
        subtitle: ["12px", "1"],
        subtitleSmall: ["11px", "1"],
        subtitleSmallExtra: ["10px", "1"],
        caption: ["12px", "1"],
        captionSmall: ["11px", "1"],

        // Heading sizes:
        heading1: ["28px", "0.8"],
        heading2: ["24px", "0.8"],
        heading3: ["20px", "1"],
        heading4: ["18px", "1"],
        heading5: ["17px", "1"],
        heading6: ["16px", "1"],

        // Paragraph sizes:
        p1: ["14px", "1.3"],
        p2: ["13px", "1.3"],
        p3: ["12px", "1.3"],
        p4: ["11px", "1.3"],
      },
      fontFamily: {
        sans: ["Inter", ...defaultTheme.fontFamily.sans],
        mono: ["Space Mono", ...defaultTheme.fontFamily.mono],
      },
      colors: {
        sui: {
          dark: "#1F6493",
          DEFAULT: "#6fbcf0",
          light: "#E1F3FF",
          steel: {
            DEFAULT: "#A0B6C3",
            dark: "#758F9E",
            darker: "#627885",
          },
          // TODO: Replace default gray palette with this:
          grey: {
            100: "#182435",
            95: "#2A3645",
            90: "#3D444D",
            85: "#4E555D",
            80: "#636870",
            75: "#767A81",
            70: "#898D93",
            65: "#9C9FA4",
            60: "#C3C5C8",
            55: "#D7D8DA",
            50: "#E9EAEB",
            45: "#F0F1F2",
            40: "#F7F8F8",
            35: "#FEFEFE",
          },
        },
        issue: {
          DEFAULT: "#FF794B",
          dark: "#EB5A29",
          light: "#FFECE5",
        },
        hero: {
          DEFAULT: "#0284AD",
          dark: "#007195",
        },
        success: {
          DEFAULT: "#2DD7A7",
          dark: "#008C65",
          light: "#D5F7EE",
        },
        warning: {
          DEFAULT: "#F2BD24",
          dark: "#8D6E15",
          light: "#FFF8E2",
        },
        headerNav: "#2A4362",
        search: {
          fill: "#162A43",
        },
        offwhite: "#fefefe",
        offblack: "#111111",
      },
    },
  },
  corePlugins: {
    aspectRatio: false,
  },
  plugins: [
    require("@headlessui/tailwindcss"),
    require("@tailwindcss/aspect-ratio"),
  ],
};
