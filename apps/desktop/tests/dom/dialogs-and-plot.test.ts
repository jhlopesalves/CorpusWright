import { beforeEach, describe, expect, it, vi } from "vitest";

import { bindModalVisibility } from "../../src/app/modal";
import { bindProjectCreationMetadataControls } from "../../src/features/project/creation-dialog";
import { attachPlotHover } from "../../src/plots/render";

describe("project creation metadata dialog", () => {
  beforeEach(() => {
    document.body.innerHTML = `
      <input id="name">
      <select id="language"><option value=""></option><option value="en">English</option></select>
      <button id="confirm">Create</button>
      <button id="cancel">Cancel</button>
      <p id="status"></p>
    `;
  });

  it("validates DOM input and submits only a complete draft", () => {
    const name = document.querySelector<HTMLInputElement>("#name")!;
    const language = document.querySelector<HTMLSelectElement>("#language")!;
    const confirm = document.querySelector<HTMLButtonElement>("#confirm")!;
    const submit = vi.fn();
    const cancel = vi.fn();
    bindProjectCreationMetadataControls(
      {
        name,
        language,
        confirm,
        cancel: document.querySelector("#cancel")!,
        status: document.querySelector("#status")!,
      },
      submit,
      cancel,
    );

    expect(confirm.disabled).toBe(true);
    name.value = "Synthetic study";
    name.dispatchEvent(new InputEvent("input", { bubbles: true }));
    expect(confirm.disabled).toBe(true);
    language.value = "en";
    language.dispatchEvent(new Event("change", { bubbles: true }));
    expect(confirm.disabled).toBe(false);
    confirm.click();
    expect(submit).toHaveBeenCalledOnce();
    document.querySelector<HTMLButtonElement>("#cancel")!.click();
    expect(cancel).toHaveBeenCalledOnce();
  });
});

describe("settings modal visibility", () => {
  it("opens and closes from both real close controls", () => {
    document.body.innerHTML = `
      <button id="open">Settings</button><section id="modal" class="hidden">
      <button id="cancel">Cancel</button><button id="top">Close</button></section>
    `;
    const modal = document.querySelector<HTMLElement>("#modal")!;
    bindModalVisibility(
      {
        modal,
        open: document.querySelector("#open")!,
        close: [document.querySelector("#cancel")!, document.querySelector("#top")!],
      },
      () => modal.classList.remove("hidden"),
      () => modal.classList.add("hidden"),
    );
    document.querySelector<HTMLButtonElement>("#open")!.click();
    expect(modal.classList.contains("hidden")).toBe(false);
    document.querySelector<HTMLButtonElement>("#top")!.click();
    expect(modal.classList.contains("hidden")).toBe(true);
  });
});

describe("figure keyboard pinning", () => {
  it("dispatches the production pin event on Enter", () => {
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    const marker = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    marker.dataset.pinKey = "document:1";
    svg.append(marker);
    document.body.append(svg);
    const toggled = vi.fn();
    svg.addEventListener("cw-plot-pin-toggle", toggled);
    attachPlotHover(svg);
    marker.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(toggled).toHaveBeenCalledOnce();
    expect((toggled.mock.calls[0]![0] as CustomEvent).detail).toEqual({ key: "document:1" });
  });
});
