
import React from "react";
import Content from "@theme-original/DocSidebar/Desktop/Content";
import SidebarIframe from "@site/src/components/SidebarIframe";

export default function ContentWrapper(props) {
  const wrapperRef = React.useRef(null);
  const scrollRef = React.useRef(null);
  const footerRef = React.useRef(null);
  const contentRef = React.useRef(null);
  const [footerHeight, setFooterHeight] = React.useState(0);
  const [showShadow, setShowShadow] = React.useState(false);

  React.useEffect(() => {
    const wrapper = wrapperRef.current;
    if (!wrapper) return;

    const scrollEl = scrollRef.current;
    if (!scrollEl) return;

    // Measure footer and pad scroll container so content doesn't hide behind it
    const measureFooter = () => {
      const footerH = footerRef.current ? footerRef.current.offsetHeight : 0;
      setFooterHeight(footerH);
      // Our gradient overlay is h-4 (1rem = 16px). Subtract it so the last item can sit just under the fade,
      // not leave extra empty space.
      const GRADIENT_PX = 16;
      const effectivePad = Math.max(footerH - GRADIENT_PX, 0);
      scrollEl.style.paddingBottom = effectivePad ? `${effectivePad}px` : "";
    };
    measureFooter();
    const footerRO = new ResizeObserver(measureFooter);
    if (footerRef.current) footerRO.observe(footerRef.current);

    const atBottom = (el, pad = 1) =>
      el.scrollTop + el.clientHeight >= el.scrollHeight - pad;

    const update = () => setShowShadow(!atBottom(scrollEl));

    // Initial check
    update();

    // Scroll + resize observers
    const onScroll = () => requestAnimationFrame(update);
    scrollEl.addEventListener("scroll", onScroll, { passive: true });

    const ro = new ResizeObserver(() => update());
    ro.observe(scrollEl);

    // Observe inner content size changes (collapsible sections expanding/collapsing)
    const contentEl = contentRef.current || scrollEl.firstElementChild;
    let contentRO;
    if (contentEl) {
      contentRO = new ResizeObserver(() => update());
      contentRO.observe(contentEl);
    }

    // Also observe content changes inside the scroller (collapsible sections)
    const mo = new MutationObserver(() => update());
    mo.observe(scrollEl, { childList: true, subtree: true, attributes: true });

    return () => {
      scrollEl.removeEventListener("scroll", onScroll);
      ro.disconnect();
      mo.disconnect();
      footerRO.disconnect();
      if (contentRO) contentRO.disconnect();
    };
  }, []);

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Scrollable content area */}
      <div ref={wrapperRef} className="relative flex-1 min-h-0">
        <div ref={scrollRef} className="relative h-full overflow-auto">
          <div ref={contentRef}>
            <Content {...props} />
          </div>
        </div>
        {/* Top-edge gradient that appears when there is more content below (nav not at bottom) */}
        <div
          aria-hidden
          className={`${
            showShadow ? "opacity-100" : "opacity-0"
          } absolute inset-x-0 bottom-0 h-4 bg-gradient-to-t from-black/20 to-transparent pointer-events-none transition-opacity duration-200 z-20`}
        />
      </div>

      {/* Bottom fixed actions */}
      <div
        ref={footerRef}
        className="shrink-0 p-2 z-10 bg-[var(--ifm-background-color)] border-t border-black/10 dark:border-white/10"
      >
        <SidebarIframe
          url="https://cal.com/forms/08983b87-8001-4df6-896a-0d7b60acfd79"
          label="Book Office Hours"
          icon="🗳️"
        />
        <SidebarIframe
          url="https://discord.gg/sui"
          label="Join Discord"
          icon="💬"
          openInNewTab={true}
        />
      </div>
    </div>
  );
}
