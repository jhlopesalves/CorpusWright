function selectTab(tabs: HTMLButtonElement[], panels: HTMLElement[], index: number) {
  tabs.forEach((tab, current) => {
    const selected = current === index;
    tab.setAttribute("aria-selected", String(selected));
    tab.tabIndex = selected ? 0 : -1;
  });
  panels.forEach((panel, current) => {
    panel.hidden = current !== index;
  });
}

export function initialiseScreenshotTabs() {
  const tablist = document.querySelector<HTMLElement>(".shot-tabs");
  if (!tablist) return;
  const tabs = [...tablist.querySelectorAll<HTMLButtonElement>('[role="tab"]')];
  const panels = [...document.querySelectorAll<HTMLElement>('.shot-panel[role="tabpanel"]')];
  if (!tabs.length || tabs.length !== panels.length) return;

  tabs.forEach((tab, index) => tab.addEventListener("click", () => selectTab(tabs, panels, index)));
  tablist.addEventListener("keydown", (event) => {
    const current = tabs.indexOf(document.activeElement as HTMLButtonElement);
    if (current < 0) return;
    let next: number | null = null;
    if (event.key === "ArrowRight") next = (current + 1) % tabs.length;
    else if (event.key === "ArrowLeft") next = (current - 1 + tabs.length) % tabs.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = tabs.length - 1;
    if (next === null) return;
    event.preventDefault();
    tabs[next].focus();
    selectTab(tabs, panels, next);
  });
  selectTab(tabs, panels, 0);
}
