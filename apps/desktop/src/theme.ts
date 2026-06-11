import {
  dom,
  getDropdownItems,
  getDropdowns,
  getLeftPanel,
} from "./dom";

export function initThemeToggle(): void {
  dom.themeToggle.addEventListener("click", () => {
    const current = document.documentElement.dataset.theme;
    document.documentElement.dataset.theme = current === "light" ? "dark" : "light";
  });
}

export function initAppChrome(): void {
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      if (!dom.settingsModal.classList.contains("hidden")) {
        dom.settingsModal.classList.add("hidden");
      }

      document.querySelectorAll(".dropdown").forEach(d => d.classList.remove("is-open"));
    }
  });

  const dropdowns = getDropdowns();

  dropdowns.forEach(dropdown => {
    const menuLabel = dropdown.querySelector(".menu-item");
    menuLabel?.addEventListener("click", (e) => {
      e.stopPropagation();
      const isOpen = dropdown.classList.contains("is-open");
      dropdowns.forEach(d => d.classList.remove("is-open"));
      if (!isOpen) {
        dropdown.classList.add("is-open");
      }
    });
  });

  document.addEventListener("click", () => {
    dropdowns.forEach(d => d.classList.remove("is-open"));
  });

  const dropdownItems = getDropdownItems();
  dropdownItems.forEach(item => {
    item.addEventListener("click", () => {
      dropdowns.forEach(d => d.classList.remove("is-open"));
    });
  });

  const splitter = dom.sidebarSplitter;
  const leftPanel = getLeftPanel();
  let isResizing = false;

  if (splitter && leftPanel) {
    splitter.addEventListener("mousedown", (e) => {
      isResizing = true;
      splitter.classList.add("active");
      document.body.style.cursor = "col-resize";
      e.preventDefault();
    });

    document.addEventListener("mousemove", (e) => {
      if (!isResizing) return;
      e.preventDefault();

      let newWidth = e.clientX;
      const maxW = Math.min(640, window.innerWidth * 0.55);

      if (newWidth < 240) newWidth = 240;
      if (newWidth > maxW) newWidth = maxW;

      leftPanel.style.width = `${newWidth}px`;
    });

    document.addEventListener("mouseup", () => {
      if (isResizing) {
        isResizing = false;
        splitter.classList.remove("active");
        document.body.style.cursor = "default";
      }
    });
  }
}
