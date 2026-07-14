export interface ModalVisibilityElements {
  modal: HTMLElement;
  open: HTMLElement;
  close: readonly HTMLElement[];
}

/** Shared mouse/keyboard-independent modal visibility binding used by settings. */
export function bindModalVisibility(
  elements: ModalVisibilityElements,
  onOpen: () => void,
  onClose: () => void,
): void {
  elements.open.addEventListener("click", onOpen);
  for (const close of elements.close) close.addEventListener("click", onClose);
}
