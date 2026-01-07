import React from "react";
import clsx from "clsx";
import { translate } from "@docusaurus/Translate";
import { useCodeBlockContext } from "@docusaurus/theme-common/internal";
import Button from "@theme/CodeBlock/Buttons/Button";

export default function WordWrapButton({ className }) {
  const { wordWrap } = useCodeBlockContext();
  const canShowButton = wordWrap.isEnabled || wordWrap.isCodeScrollable;
  if (!canShowButton) {
    return false;
  }
  const title = translate({
    id: "theme.CodeBlock.wordWrapToggle",
    message: "Toggle word wrap",
    description:
      "The title attribute for toggle word wrapping button of code block lines",
  });
  return (
    <Button
      onClick={() => wordWrap.toggle()}
      className={clsx(className, "text-xs !opacity-40 w-24 justify-center")}
      aria-label={title}
      title={title}
    >
      <i class="fa-solid fa-chart-bar leading-[0] pr-1"></i>
      {wordWrap.isEnabled ? "No wrap" : "Wrap"}
    </Button>
  );
}
