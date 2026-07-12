function selectedTheme(): "dark" | "light" {
  return document.documentElement.dataset.theme === "dark" ? "dark" : "light";
}

function synchroniseToggle(toggle: HTMLButtonElement) {
  const dark = selectedTheme() === "dark";
  toggle.setAttribute("aria-pressed", String(dark));
  toggle.setAttribute("aria-label", dark ? "Switch to light theme" : "Switch to dark theme");
}

export function initialiseTheme() {
  const toggle = document.querySelector<HTMLButtonElement>("#theme-toggle");
  if (!toggle) return;
  synchroniseToggle(toggle);
  toggle.addEventListener("click", () => {
    const next = selectedTheme() === "dark" ? "light" : "dark";
    document.documentElement.dataset.theme = next;
    try {
      window.localStorage.setItem("theme", next);
    } catch {
      // The selected theme still applies for this page when storage is unavailable.
    }
    synchroniseToggle(toggle);
  });
}
