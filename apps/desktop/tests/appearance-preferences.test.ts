type Listener = (event: { key?: string; type: string }) => void;

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

class FakeClassList {
  private readonly tokens = new Set<string>();

  add(token: string): void {
    this.tokens.add(token);
  }

  remove(token: string): void {
    this.tokens.delete(token);
  }

  contains(token: string): boolean {
    return this.tokens.has(token);
  }
}

class FakeStyle {
  readonly values = new Map<string, string>();

  setProperty(name: string, value: string): void {
    this.values.set(name, value);
  }
}

class FakeElement {
  readonly classList = new FakeClassList();
  readonly dataset: Record<string, string> = {};
  readonly listeners = new Map<string, Listener[]>();
  readonly style = new FakeStyle();
  children: FakeElement[] = [];
  textContent = "";
  value = "";
  valueAsNumber = Number.NaN;
  readonly id: string;

  constructor(id: string) {
    this.id = id;
  }

  addEventListener(type: string, listener: Listener): void {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }

  replaceChildren(...children: FakeElement[]): void {
    this.children = children;
  }

  focus(): void {}

  dispatch(type: string, event: { key?: string; type: string } = { type }): void {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(event);
    }
  }
}

class FakeStorage {
  private readonly values = new Map<string, string>();

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }
}

const elements = new Map<string, FakeElement>();
const documentListeners = new Map<string, Listener[]>();
const root = new FakeElement("html");

function element(id: string): FakeElement {
  let existing = elements.get(id);
  if (!existing) {
    existing = new FakeElement(id);
    if (id === "appearance-modal") {
      existing.classList.add("hidden");
    }
    elements.set(id, existing);
  }
  return existing;
}

function documentAddEventListener(type: string, listener: Listener): void {
  const listeners = documentListeners.get(type) ?? [];
  listeners.push(listener);
  documentListeners.set(type, listeners);
}

globalThis.document = {
  documentElement: root,
  getElementById: element,
  querySelector: () => new FakeElement("query"),
  querySelectorAll: () => [],
  createElement: (tagName: string) => new FakeElement(tagName),
  addEventListener: documentAddEventListener,
} as unknown as Document;

globalThis.localStorage = new FakeStorage() as unknown as Storage;

const appearance = await import("../src/appearance.ts");
const key = appearance.APPEARANCE_STORAGE_KEY;
const gentium = appearance.FONT_OPTIONS.find(font => font.id === "gentium-plus");
const roboto = appearance.FONT_OPTIONS.find(font => font.id === "roboto");
const firaCode = appearance.MONO_FONT_OPTIONS.find(font => font.id === "fira-code");
assert(gentium, "Gentium Plus must be in the fixed font shortlist");
assert(roboto, "Roboto must be in the fixed font shortlist");
assert(firaCode, "Fira Code must be in the fixed monospace shortlist");

appearance.initAppearanceDialog();
assert(root.dataset.theme === "dark", "initial appearance must apply the default dark theme");
assert(root.style.values.get("--cw-font-size-base") === "16px", "initial appearance must apply the default font size");

element("menu-appearance").dispatch("click");
element("appearance-theme-select").value = "solar-paper";
element("appearance-theme-select").dispatch("change");
element("appearance-font-select").value = "gentium-plus";
element("appearance-font-select").dispatch("change");
element("appearance-mono-font-select").value = "fira-code";
element("appearance-mono-font-select").dispatch("change");
element("appearance-font-size-input").valueAsNumber = 18;
element("appearance-font-size-input").dispatch("input");
assert(root.dataset.theme === "solar-paper", "theme selection must preview immediately");
assert(root.style.values.get("--cw-font-ui") === gentium.stack, "font selection must preview immediately");
assert(root.style.values.get("--cw-font-mono") === firaCode.stack, "monospace selection must preview immediately");
assert(root.style.values.get("--cw-font-size-base") === "18px", "font size must preview immediately");
element("appearance-save-button").dispatch("click");

let saved = JSON.parse(localStorage.getItem(key) ?? "{}");
assert(saved.themeId === "solar-paper", "saved appearance must persist the selected theme");
assert(saved.fontFamily === gentium.stack, "saved appearance must persist the selected font stack");
assert(saved.monoFontFamily === firaCode.stack, "saved appearance must persist the selected monospace stack");
assert(saved.fontSize === 18, "saved appearance must persist the selected font size");

element("menu-appearance").dispatch("click");
element("appearance-theme-select").value = "high-contrast";
element("appearance-theme-select").dispatch("change");
assert(root.dataset.theme === "high-contrast", "cancelled edits must still preview before cancellation");
element("appearance-cancel-button").dispatch("click");
assert(root.dataset.theme === "solar-paper", "cancel must restore the last saved theme");

appearance.toggleLightDarkTheme();
saved = JSON.parse(localStorage.getItem(key) ?? "{}");
assert(saved.themeId === "light", "quick toggle must persist the light default from a non-light theme");
assert(root.dataset.theme === "light", "quick toggle must apply the light default immediately");

appearance.toggleLightDarkTheme();
saved = JSON.parse(localStorage.getItem(key) ?? "{}");
assert(saved.themeId === "dark", "quick toggle must persist the dark default from light");
assert(root.dataset.theme === "dark", "quick toggle must apply the dark default immediately");

element("menu-appearance").dispatch("click");
element("appearance-font-size-input").valueAsNumber = 99;
element("appearance-font-size-input").dispatch("input");
assert(root.style.values.get("--cw-font-size-base") === "20px", "font size preview must clamp to the upper bound");
element("appearance-reset-button").dispatch("click");
element("appearance-save-button").dispatch("click");

saved = JSON.parse(localStorage.getItem(key) ?? "{}");
assert(saved.themeId === "dark", "reset must restore the default theme");
assert(saved.fontSize === 16, "reset must restore the default font size");
