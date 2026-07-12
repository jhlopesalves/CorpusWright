const scrollableSelector = "pre.code, .math-display, .learn-prose p mjx-container";

function synchroniseScrollableRegions() {
  for (const element of document.querySelectorAll<HTMLElement>(scrollableSelector)) {
    if (element.scrollWidth > element.clientWidth + 1 || element.scrollHeight > element.clientHeight + 1) {
      element.tabIndex = 0;
    }
  }
}

export function initialiseScrollableRegions() {
  synchroniseScrollableRegions();
  const concepts = document.querySelector(".learn-prose");
  if (concepts) {
    const observer = new MutationObserver(() => window.requestAnimationFrame(synchroniseScrollableRegions));
    observer.observe(concepts, { childList: true, subtree: true });
    window.addEventListener("load", () => {
      synchroniseScrollableRegions();
      observer.disconnect();
    }, { once: true });
  }
}
