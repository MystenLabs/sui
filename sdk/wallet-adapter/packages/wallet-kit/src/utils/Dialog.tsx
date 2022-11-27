// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Dialog from "@radix-ui/react-dialog";
import { styled } from "../stitches";

export const Title = styled(Dialog.Title, {
  margin: 0,
  padding: "0 $2",
  fontSize: "$lg",
  fontWeight: "$title",
  color: "$textDark",
});

export const Overlay = styled(Dialog.Overlay, {
  backgroundColor: "$backdrop",
  position: "fixed",
  inset: 0,
});

export const Content = styled(Dialog.Content, {
  position: "fixed",
  inset: 0,
  zIndex: 100,
  height: "100%",
  fontFamily: "$sans",
  display: "flex",
  justifyContent: "center",
  alignItems: "flex-end",
  padding: "$4",
  boxSizing: "border-box",

  "@md": {
    alignItems: "center",
  },
});

export const Body = styled("div", {
  position: "relative",
  overflow: "hidden",
  backgroundColor: "$background",
  borderRadius: "$modal",
  boxShadow: "$modal",
  width: "100%",
  minHeight: "50vh",
  maxWidth: "700px",
  maxHeight: "85vh",
  display: "flex",
  flexDirection: 'column',

  "@md": {
    flexDirection: 'row'
  },
});
