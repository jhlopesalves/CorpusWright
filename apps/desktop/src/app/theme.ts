import {
  dom,
  getLeftPanel,
} from "./dom";
import { toggleLightDarkTheme } from "./appearance";

export function initThemeToggle(): void {
  dom.themeToggle.addEventListener("click", toggleLightDarkTheme);
  dom.menuToggleTheme.addEventListener("click", toggleLightDarkTheme);
}

export function closeAppMenu(restoreFocus = false): void {
  if (dom.appMenu.hidden) return;
  dom.appMenu.hidden = true;
  dom.appMenuButton.setAttribute("aria-expanded", "false");
  if (restoreFocus) dom.appMenuButton.focus();
}

export function initAppChrome(): void {
  dom.appMenuButton.addEventListener("click", () => {
    if (!dom.appMenu.hidden) {
      closeAppMenu();
      return;
    }
    dom.appMenu.hidden = false;
    dom.appMenuButton.setAttribute("aria-expanded", "true");
  });

  dom.appMenu.addEventListener("click", (event) => {
    const item = (event.target as Element).closest("button");
    if (item && item.getAttribute("aria-disabled") !== "true") {
      closeAppMenu(dom.appMenu.contains(document.activeElement));
    }
  });

  document.addEventListener("click", (event) => {
    const target = event.target as Node;
    if (!dom.appMenu.contains(target) && !dom.appMenuButton.contains(target)) closeAppMenu();
  });

  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    closeAppMenu(dom.appMenu.contains(document.activeElement));
    if (!dom.settingsModal.classList.contains("hidden")) {
      dom.settingsModal.classList.add("hidden");
    }
  });

  dom.railAppearance.addEventListener("click", () => dom.menuAppearance.click());
  dom.railAbout.addEventListener("click", () => dom.menuAboutCorpusWright.click());

  const splitter = dom.sidebarSplitter;
  const leftPanel = getLeftPanel();
  let isResizing = false;

  if (splitter && leftPanel) {
    splitter.addEventListener("mousedown", (e) => {
      isResizing = true;
      splitter.classList.add("active");
      document.body.classList.add("is-resizing-sidebar");
      e.preventDefault();
    });

    document.addEventListener("mousemove", (e) => {
      if (!isResizing) return;
      e.preventDefault();

      let newWidth = e.clientX;
      const maxW = Math.min(640, window.innerWidth * 0.55);

      if (newWidth < 240) newWidth = 240;
      if (newWidth > maxW) newWidth = maxW;

      leftPanel.style.setProperty("--cw-sidebar-width", `${newWidth}px`);
    });

    document.addEventListener("mouseup", () => {
      if (isResizing) {
        isResizing = false;
        splitter.classList.remove("active");
        document.body.classList.remove("is-resizing-sidebar");
      }
    });
  }
}
