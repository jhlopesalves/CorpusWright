import { dom } from "./dom";

let returnFocusTarget: HTMLElement | null = null;

function openAboutModal(): void {
  returnFocusTarget = document.activeElement instanceof HTMLElement
    ? document.activeElement
    : dom.menuAboutCorpusWright;

  dom.aboutModal.classList.remove("hidden");
  dom.btnCloseAboutModalTop.focus();
}

function closeAboutModal(): void {
  if (dom.aboutModal.classList.contains("hidden")) return;

  dom.aboutModal.classList.add("hidden");
  const focusTarget = returnFocusTarget ?? dom.menuAboutCorpusWright;
  returnFocusTarget = null;
  focusTarget.focus();
}

export function initAboutModal(): void {
  dom.menuAboutCorpusWright.addEventListener("click", openAboutModal);
  dom.btnCloseAboutModal.addEventListener("click", closeAboutModal);
  dom.btnCloseAboutModalTop.addEventListener("click", closeAboutModal);

  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    if (dom.aboutModal.classList.contains("hidden")) return;

    event.preventDefault();
    closeAboutModal();
  });
}
