import React from "react";
import clsx from "clsx";
import { useCodeBlockContext } from "@docusaurus/theme-common/internal";
import { usePrismTheme } from "@docusaurus/theme-common";
import { Highlight } from "prism-react-renderer";
import Line from "@theme/CodeBlock/Line";
import styles from "./styles.module.css";
// TODO Docusaurus v4: remove useless forwardRef
const Pre = React.forwardRef((props, ref) => {
  return (
    <pre
      ref={ref}
      /* eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex */
      tabIndex={0}
      {...props}
      className={clsx(props.className, styles.codeBlock, "thin-scrollbar")}
    />
  );
});
function Code(props) {
  const { metadata } = useCodeBlockContext();
  return (
    <code
      {...props}
      className={clsx(
        props.className,
        styles.codeBlockLines,
        metadata.lineNumbersStart !== undefined &&
          styles.codeBlockLinesWithNumbering,
      )}
      style={{
        ...props.style,
        counterReset:
          metadata.lineNumbersStart === undefined
            ? undefined
            : `line-count ${metadata.lineNumbersStart - 1}`,
      }}
    />
  );
}
const applyTransforms = (lines) => {
  const out = [];

  for (let i = 0; i < lines.length; i++) {
    let tokens = lines[i];

    if (tokens[0]?.content?.startsWith("$ ")) {
      const promptToken = {
        types: ["select-none", "opacity-40", "text-lg"],
        content: "$ ",
      };
      const first = tokens[0];
      const restOfFirst = { ...first, content: first.content.slice(2) };
      tokens = [promptToken, restOfFirst, ...tokens.slice(1)];
    }

    out.push(tokens);
  }

  return out;
};
export default function CodeBlockContent({ className: classNameProp }) {
  const { metadata, wordWrap } = useCodeBlockContext();
  const prismTheme = usePrismTheme();
  const { code, language, lineNumbersStart, lineClassNames } = metadata;
  return (
    <Highlight theme={prismTheme} code={code} language={language}>
      {({
        className,
        style,
        tokens: rawLines,
        getLineProps,
        getTokenProps,
      }) => {
        const lines = applyTransforms(rawLines);
        return (
          <Pre
            ref={wordWrap.codeBlockRef}
            className={clsx(classNameProp, className, "!pt-[14px]")}
            style={style}
          >
            <Code>
              {lines.map((line, i) => (
                <Line
                  key={i}
                  line={line}
                  getLineProps={getLineProps}
                  getTokenProps={getTokenProps}
                  classNames={lineClassNames[i]}
                  showLineNumbers={lineNumbersStart !== undefined}
                />
              ))}
            </Code>
          </Pre>
        );
      }}
    </Highlight>
  );
}
