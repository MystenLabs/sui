import type { ThemeVars } from './themeContract.js';

export const darkTheme: ThemeVars = {
  blurs: {
    modalOverlay: "blur(0)",
  },
  backgroundColors: {
    primaryButton: "#373737",
    primaryButtonHover: "#2C2C2C",
    outlineButtonHover: "#2C2C2C",
    modalOverlay: "rgba(255, 255, 255, 0.1)",
    modalPrimary: "#373737",
    modalSecondary: "#2C2C2C",
    iconButton: "transparent",
    iconButtonHover: "#2C2C2C",
    dropdownMenu: "#373737",
    dropdownMenuSeparator: "#2C2C2C",
    walletItemSelected: "#373737",
    walletItemHover: "#2C2C2C",
  },
  borderColors: {
    outlineButton: "#2C2C2C",
  },
  colors: {
    primaryButton: "#F6F7F9",
    outlineButton: "#F6F7F9",
    iconButton: "#F6F7F9",
    body: "#F6F7F9",
    bodyMuted: "#E4E4E7",
    bodyDanger: "#FF794B",
  },
  radii: {
    small: "6px",
    medium: "8px",
    large: "12px",
    xlarge: "16px",
  },
  shadows: {
    primaryButton: "0px 4px 12px rgba(0, 0, 0, 0.5)",
    walletItemSelected: "0px 2px 6px rgba(0, 0, 0, 0.5)",
  },
  fontWeights: {
    normal: "400",
    medium: "500",
    bold: "600",
  },
  fontSizes: {
    small: "14px",
    medium: "16px",
    large: "18px",
    xlarge: "20px",
  },
  typography: {
    fontFamily:
      'ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", sans-serif, "Apple Color Emoji", "Segoe UI Emoji", "Segoe UI Symbol", "Noto Color Emoji"',
    fontStyle: "normal",
    lineHeight: "1.3",
    letterSpacing: "1",
  },
};
