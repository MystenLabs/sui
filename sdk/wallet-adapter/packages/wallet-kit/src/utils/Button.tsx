import { styled } from "../stitches";

export const Button = styled("button", {
  cursor: "pointer",
  border: "none",
  fontFamily: "$sans",
  fontWeight: "$button",
  fontSize: "$sm",

  variants: {
    size: {
      md: {
        padding: "$2 $4",
        borderRadius: "$buttonMd",
      },
      lg: {
        padding: "$4 $6",
        borderRadius: "$buttonLg",
      },
    },
    color: {
      primary: {
        backgroundColor: "$brand",
        color: "$textOnBrand",
        "&:hover": {
          backgroundColor: "$brandAccent",
        },
        boxShadow: "$button",
      },
      secondary: {
        backgroundColor: "transparent",
        border: "1px solid $secondary",
        color: "$secondaryAccent",
      },
      connected: {
        boxShadow: "$button",
        backgroundColor: "$background",
        color: "$textDark",
      },
    },
  },
  defaultVariants: {
    size: "md",
  },
});
