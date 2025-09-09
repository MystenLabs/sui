// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React, {
  useCallback,
  useState,
  useRef,
  useEffect,
  type ReactNode,
} from "react";
import clsx from "clsx";
import copy from "copy-text-to-clipboard";
import { translate } from "@docusaurus/Translate";
import Button from "@theme/CodeBlock/Buttons/Button";
import type { Props } from "@theme/CodeBlock/Buttons/CopyButton";
import IconCopy from "@theme/Icon/Copy";
import IconSuccess from "@theme/Icon/Success";

import styles from "./styles.module.css";

function getNearestCodeText(start: HTMLElement | null): string | null {
  let el: HTMLElement | null = start;
  while (el) {
    // Try common code selectors within a code block
    const codeEl = el.querySelector?.('pre code, code, pre') as HTMLElement | null;
    if (codeEl && codeEl.innerText) {
      return codeEl.innerText;
    }
    el = el.parentElement;
  }
  return null;
}

function title() {
  return translate({
    id: "theme.CodeBlock.copy",
    message: "Copy",
    description: "The copy button label on code blocks",
  });
}

function ariaLabel(isCopied: boolean) {
  return isCopied
    ? translate({
        id: "theme.CodeBlock.copied",
        message: "Copied",
        description: "The copied button label on code blocks",
      })
    : translate({
        id: "theme.CodeBlock.copyButtonAriaLabel",
        message: "Copy code to clipboard",
        description: "The ARIA label for copy code blocks button",
      });
}

function useCopyButton(buttonRef: React.RefObject<HTMLElement>) {
  const [isCopied, setIsCopied] = useState(false);
  const copyTimeout = useRef<number | undefined>(undefined);

  const copyCode = useCallback(() => {
    const text = getNearestCodeText(buttonRef.current ?? null);
    if (!text) return;
    const cleaned = text.replace(/^\$ /gm, "").replace(/\n$/, "");
    copy(cleaned);
    setIsCopied(true);
    copyTimeout.current = window.setTimeout(() => {
      setIsCopied(false);
    }, 1000);
  }, [buttonRef]);

  useEffect(() => () => window.clearTimeout(copyTimeout.current), []);

  return { copyCode, isCopied };
}

export default function CopyButton({ className }: Props): ReactNode {
  const buttonRef = useRef<HTMLSpanElement | null>(null);
  const { copyCode, isCopied } = useCopyButton(buttonRef);

  return (
    <span ref={buttonRef}>
      <Button
        aria-label={ariaLabel(isCopied)}
        title={title()}
        className={clsx(
          className,
          styles.copyButton,
          isCopied && styles.copyButtonCopied,
        )}
        onClick={copyCode}
      >
        <span className={styles.copyButtonIcons} aria-hidden="true">
          <IconCopy className={styles.copyButtonIcon} />
          <IconSuccess className={styles.copyButtonSuccessIcon} />
        </span>
      </Button>
    </span>
  );
}
