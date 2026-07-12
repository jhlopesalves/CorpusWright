export function initialiseDocumentationToc() {
  const toc = document.querySelector<HTMLElement>("[data-docs-toc]");
  if (!toc || !("IntersectionObserver" in window)) return;
  const entries = [...toc.querySelectorAll<HTMLAnchorElement>('a[href^="#"]')]
    .map((link) => {
      let id: string;
      try {
        id = decodeURIComponent(link.hash.slice(1));
      } catch {
        return null;
      }
      const heading = document.getElementById(id);
      return heading ? { link, heading } : null;
    })
    .filter((entry): entry is { link: HTMLAnchorElement; heading: HTMLElement } => entry !== null);
  if (!entries.length) return;

  const setActive = (activeLink: HTMLAnchorElement) => {
    for (const { link } of entries) {
      const active = link === activeLink;
      link.classList.toggle("is-active", active);
      if (active) link.setAttribute("aria-current", "true");
      else link.removeAttribute("aria-current");
    }
  };
  const observer = new IntersectionObserver((observed) => {
    const visible = observed.filter((entry) => entry.isIntersecting)
      .sort((left, right) => left.boundingClientRect.top - right.boundingClientRect.top);
    const active = visible.length ? entries.find((entry) => entry.heading === visible[0].target) : null;
    if (active) setActive(active.link);
  }, { rootMargin: "-82px 0px -70% 0px", threshold: [0, 1] });
  for (const entry of entries) {
    observer.observe(entry.heading);
    entry.link.addEventListener("click", () => setActive(entry.link));
  }
  setActive(entries[0].link);
}
