# Prompt: Add "Open in Move Playground" button to Docusaurus code blocks

Add an inline "Playground" button to the CodeBlock button bar (next to Copy and Use an Agent) that opens the code in PlayMove (https://www.playmove.dev) in a new tab. The button only appears on Move code blocks.

---

## How it works

PlayMove reads code from `window.location.hash`. Opening `https://www.playmove.dev/#<encodeURIComponent(code)>` pre-populates the editor with the given code.

The button is added to the swizzled `CopyButton` component, which already renders the Copy and Open in Agent buttons inside the CodeBlock button group. It detects Move code blocks by walking the DOM upward to find `pre code[class*='language-move']`.

---

## Implementation

### Modify the swizzled CopyButton

**File:** `site/src/theme/CodeBlock/Buttons/CopyButton/index.tsx`

This file already exports a `CopyButton` component that renders inside every code block's button group. Add an `OpenInPlayMoveButton` component to the same file.

#### 1. Add the OpenInPlayMoveButton component

Add this component before the `CopyButton` export:

```tsx
function OpenInPlayMoveButton({ className }: { className?: string }) {
  const wrapperRef = useRef<HTMLSpanElement | null>(null);
  const [isMove, setIsMove] = useState(false);

  // Detect if this code block uses Move language
  useEffect(() => {
    let el: HTMLElement | null = wrapperRef.current;
    while (el) {
      const code = el.querySelector?.("pre code[class*='language-move']") as HTMLElement | null;
      if (code) {
        setIsMove(true);
        return;
      }
      el = el.parentElement;
    }
  }, []);

  // Get code text by walking the DOM (reuse the existing getNearestCodeText helper)
  const handleClick = useCallback(() => {
    const code = getNearestCodeText(wrapperRef.current);
    if (!code) return;
    const url = `https://www.playmove.dev/#${encodeURIComponent(code)}`;
    window.open(url, "_blank", "noopener");
  }, []);

  // Always render the wrapper span so the ref is set on mount.
  // Toggle display based on whether this is a Move code block.
  return (
    <span ref={wrapperRef} style={{ display: isMove ? "contents" : "none" }}>
      <Button
        aria-label="Open in Move Playground"
        title="Open in Move Playground"
        className={clsx(
          className,
          "!opacity-50 !hover:opacity-100 text-xs !pb-2 justify-center",
        )}
        onClick={handleClick}
      >
        <span className="p-1">
          <i className="fa-solid fa-play leading-[0] pr-1" style={{ fontSize: 9 }}></i>Playground
        </span>
      </Button>
    </span>
  );
}
```

**Key details:**

- The wrapper `<span ref={wrapperRef}>` must always render (not conditionally returned) so the ref is available when `useEffect` runs on mount.
- `display: "contents"` makes the span invisible in layout when visible, `display: "none"` hides it entirely for non-Move blocks.
- `getNearestCodeText` is a helper that already exists in the CopyButton file. It walks up the DOM from the given element, and at each ancestor tries `querySelector("pre code, code, pre")` to find the code text content.
- `Button` is imported from `@theme/CodeBlock/Buttons/Button` (already imported in the file).
- `clsx` is already imported in the file.
- `useState`, `useEffect`, `useCallback`, `useRef` are already imported.

#### 2. Add it to the CopyButton render

In the `CopyButton` default export, add `<OpenInPlayMoveButton />` after the existing buttons:

```tsx
export default function CopyButton({ className }: Props): ReactNode {
  // ... existing code ...
  return (
    <span ref={buttonRef} style={{ display: "contents" }}>
      <Button /* ... existing Copy button ... */ />
      <OpenInAgentButton className={className} />
      <OpenInPlayMoveButton className={className} />  {/* ADD THIS LINE */}
    </span>
  );
}
```

---

## That's it

No other files need to change. The button automatically appears on every Move code block site-wide, whether rendered by `ImportContent`, inline MDX code fences, or any other component that uses `CodeBlock` with `language="move"`.

---

## If migrating from the iframe-based PlayMoveEmbed

If you previously had a `PlayMoveEmbed` component that rendered an iframe to PlayMove:

1. **Remove the PlayMoveEmbed component** (`site/src/shared/components/PlayMoveEmbed/`) — no longer needed.

2. **Remove the conditional in ImportContent** that routed Move code to PlayMoveEmbed:
   ```tsx
   // REMOVE this block:
   if (resolvedLanguage === "move") {
     return <PlayMoveEmbed code={out} title={noTitle ? undefined : title} />;
   }
   ```

   Just let Move code fall through to the normal `CodeBlock` render path.

3. **Remove the PlayMoveEmbed import** from ImportContent.

4. **Remove PlayMoveEmbed CSS** (`site/src/shared/components/PlayMoveEmbed/styles.css`).

The inline button in the CodeBlock button bar replaces all of this with zero MDX changes needed.
