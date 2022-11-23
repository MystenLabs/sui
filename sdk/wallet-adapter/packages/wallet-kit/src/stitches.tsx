import { createStitches } from "@stitches/react";

const BASE_UNIT = 4;
const makeSize = (amount: number) => `${amount * BASE_UNIT}px`;

export const {
  styled,
  css,
  globalCss,
  keyframes,
  getCssText,
  theme,
  createTheme,
  config,
} = createStitches({
  theme: {
    colors: {
      brand: "#0284AD",
      brandDark: "#007195",
      secondary: "#A0B6C3",
      secondaryDark: "#758F9E",
      textDark: "#182435",
      textLight: "#767A81",
      textOnBrand: "#fff",
      background: "#fff",
      backdrop: "rgba(24 36 53 / 20%)",
      backgroundIcon: "#F0F1F2",
      icon: "#383F47",
    },
    space: {
      1: makeSize(1),
      2: makeSize(2),
      3: makeSize(3),
      4: makeSize(4),
      5: makeSize(5),
      6: makeSize(6),
      7: makeSize(7),
      8: makeSize(8),
    },
    fontSizes: {
      xs: "13px",
      sm: "14px",
      md: "16px",
      lg: "18px",
      xl: "20px",
    },
    fonts: {
      sans: 'ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", sans-serif, "Apple Color Emoji", "Segoe UI Emoji", "Segoe UI Symbol", "Noto Color Emoji"',
    },
    radii: {
      modal: "16px",
      button: "12px",
      wallet: "8px",
      walletIcon: "6px",
      close: "9999px",
    },
    fontWeights: {
      copy: 500,
      button: 600,
      title: 600,
    },
    sizes: {
      walletIcon: "28px",
    },
    transitions: {},
    shadows: {},
  },
});
