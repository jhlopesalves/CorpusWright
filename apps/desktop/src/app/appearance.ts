import { dom } from "./dom.ts";
import { invoke } from "./system-ipc.ts";

export const APPEARANCE_STORAGE_KEY = "corpuswright.appearance.v1";

const SYSTEM_UI_STACK = 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif';
const DEFAULT_MONO_STACK = '"JetBrains Mono", "Cascadia Mono", "Segoe UI Mono", Consolas, ui-monospace, monospace';
const DEFAULT_FONT_SIZE = 16;
const MIN_FONT_SIZE = 12;
const MAX_FONT_SIZE = 20;

export type ThemeVariant = "light" | "dark";

export interface ThemeFamily {
  id: string;
  name: string;
}

interface FontOption {
  id: string;
  name: string;
  stack: string;
}

export interface AppearancePreference {
  themeFamilyId: string;
  themeVariant: ThemeVariant;
  fontFamily: string;
  monoFontFamily: string;
  fontSize: number;
}

export const THEME_FAMILIES: ThemeFamily[] = [
  { id: "corpus", name: "Corpus" },
  { id: "solar", name: "Solar" },
  { id: "graphite", name: "Graphite" },
  { id: "retro", name: "Retro" },
  { id: "sepia", name: "Sepia" },
  { id: "moss", name: "Moss" },
  { id: "high-contrast", name: "High Contrast" },
];

export const FONT_OPTIONS: FontOption[] = [
  { id: "system-ui", name: "System UI", stack: SYSTEM_UI_STACK },
  { id: "inter", name: "Inter", stack: '"Inter", ' + SYSTEM_UI_STACK },
  { id: "roboto", name: "Roboto", stack: '"Roboto", ' + SYSTEM_UI_STACK },
  { id: "ibm-plex-sans", name: "IBM Plex Sans", stack: '"IBM Plex Sans", ' + SYSTEM_UI_STACK },
  { id: "source-sans-3", name: "Source Sans 3", stack: '"Source Sans 3", ' + SYSTEM_UI_STACK },
  { id: "noto-sans", name: "Noto Sans", stack: '"Noto Sans", ' + SYSTEM_UI_STACK },
  { id: "fira-sans", name: "Fira Sans", stack: '"Fira Sans", ' + SYSTEM_UI_STACK },
  { id: "open-sans", name: "Open Sans", stack: '"Open Sans", ' + SYSTEM_UI_STACK },
  { id: "atkinson-hyperlegible", name: "Atkinson Hyperlegible", stack: '"Atkinson Hyperlegible", ' + SYSTEM_UI_STACK },
  { id: "lexend", name: "Lexend", stack: '"Lexend", ' + SYSTEM_UI_STACK },
  { id: "source-serif-4", name: "Source Serif 4", stack: '"Source Serif 4", Georgia, serif' },
  { id: "noto-serif", name: "Noto Serif", stack: '"Noto Serif", Georgia, serif' },
  { id: "literata", name: "Literata", stack: '"Literata", Georgia, serif' },
  { id: "gentium-plus", name: "Gentium Plus", stack: '"Gentium Plus", Gentium, Georgia, serif' },
  { id: "charis-sil", name: "Charis SIL", stack: '"Charis SIL", "Gentium Plus", Georgia, serif' },
];

export const MONO_FONT_OPTIONS: FontOption[] = [
  { id: "jetbrains-mono", name: "JetBrains Mono", stack: DEFAULT_MONO_STACK },
  { id: "fira-code", name: "Fira Code", stack: '"Fira Code", ' + DEFAULT_MONO_STACK },
];

const SUN_ICON = `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><circle cx="12" cy="12" r="5"></circle><line x1="12" y1="1" x2="12" y2="3"></line><line x1="12" y1="21" x2="12" y2="23"></line><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"></line><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"></line><line x1="1" y1="12" x2="3" y2="12"></line><line x1="21" y1="12" x2="23" y2="12"></line><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"></line><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"></line></svg>`;
const MOON_ICON = `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path d="M21 12.8A9 9 0 1 1 11.2 3 7 7 0 0 0 21 12.8Z"></path></svg>`;

function preferredDefaultThemeVariant(): ThemeVariant {
  if (
    typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-color-scheme: light)").matches
  ) {
    return "light";
  }
  return "dark";
}

function createDefaultAppearance(): AppearancePreference {
  return {
    themeFamilyId: "corpus",
    themeVariant: preferredDefaultThemeVariant(),
    fontFamily: SYSTEM_UI_STACK,
    monoFontFamily: DEFAULT_MONO_STACK,
    fontSize: DEFAULT_FONT_SIZE,
  };
}

const DEFAULT_APPEARANCE: AppearancePreference = createDefaultAppearance();

let savedAppearance = readAppearancePreference();
let draftAppearance = { ...savedAppearance };

function clampFontSize(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_FONT_SIZE;
  return Math.min(MAX_FONT_SIZE, Math.max(MIN_FONT_SIZE, Math.round(value)));
}

function isKnownThemeFamily(familyId: string): boolean {
  return THEME_FAMILIES.some(family => family.id === familyId);
}

function oppositeMode(mode: ThemeVariant): ThemeVariant {
  return mode === "light" ? "dark" : "light";
}

function setThemeToggleIcon(targetMode: ThemeVariant): void {
  dom.themeToggle.innerHTML = targetMode === "light" ? SUN_ICON : MOON_ICON;
}

function refreshLightDarkToggleState(currentVariant: ThemeVariant): void {
  const targetMode = oppositeMode(currentVariant);
  const label = `Switch to ${targetMode} variant`;

  dom.themeToggle.disabled = false;
  dom.themeToggle.title = label;
  dom.themeToggle.setAttribute("aria-label", label);
  dom.menuToggleTheme.textContent = label;
  dom.menuToggleTheme.title = label;
  dom.menuToggleTheme.removeAttribute("aria-disabled");
  dom.menuToggleTheme.tabIndex = 0;

  setThemeToggleIcon(targetMode);
}

function isKnownFontStack(fontFamily: string): boolean {
  return FONT_OPTIONS.some(font => font.stack === fontFamily);
}

function isKnownMonoFontStack(fontFamily: string): boolean {
  return MONO_FONT_OPTIONS.some(font => font.stack === fontFamily);
}

function normaliseAppearance(value: any): AppearancePreference {
  const themeFamilyId = typeof value.themeFamilyId === "string" && isKnownThemeFamily(value.themeFamilyId)
    ? value.themeFamilyId
    : DEFAULT_APPEARANCE.themeFamilyId;

  const themeVariant = value.themeVariant === "light" || value.themeVariant === "dark"
    ? value.themeVariant
    : DEFAULT_APPEARANCE.themeVariant;

  const fontFamily = typeof value.fontFamily === "string" && isKnownFontStack(value.fontFamily)
    ? value.fontFamily
    : DEFAULT_APPEARANCE.fontFamily;

  const monoFontFamily = typeof value.monoFontFamily === "string" && isKnownMonoFontStack(value.monoFontFamily)
    ? value.monoFontFamily
    : DEFAULT_APPEARANCE.monoFontFamily;

  const fontSize = clampFontSize(Number(value.fontSize));

  return { themeFamilyId, themeVariant, fontFamily, monoFontFamily, fontSize };
}

function migrateLegacyAppearance(raw: any): Partial<AppearancePreference> {
  if (typeof raw.themeFamilyId === "string") {
    return raw;
  }

  const legacyId = typeof raw.themeId === "string" ? raw.themeId : "dark";
  const migrationMap: Record<string, { family: string, variant: ThemeVariant }> = {
    "dark": { family: "corpus", variant: "dark" },
    "light": { family: "corpus", variant: "light" },
    "repository-light": { family: "corpus", variant: "light" },
    "repository-dark": { family: "corpus", variant: "dark" },
    "solar-paper": { family: "solar", variant: "light" },
    "solar-night": { family: "solar", variant: "dark" },
    "graphite-day": { family: "graphite", variant: "light" },
    "graphite-desk": { family: "graphite", variant: "dark" },
    "retro-paper": { family: "retro", variant: "light" },
    "retro-console": { family: "retro", variant: "dark" },
    "sepia-reading": { family: "sepia", variant: "light" },
    "high-contrast": { family: "high-contrast", variant: "dark" },
    "moss-day": { family: "moss", variant: "light" },
    "moss-desk": { family: "moss", variant: "dark" },
    "clear-day": { family: "corpus", variant: "light" },
    "clear-night": { family: "corpus", variant: "dark" },
    "spectrum-paper": { family: "solar", variant: "light" },
    "spectrum-ink": { family: "solar", variant: "dark" },
    "cloud-desk": { family: "corpus", variant: "light" },
    "cloud-night": { family: "corpus", variant: "dark" },
    "amber-paper": { family: "sepia", variant: "light" },
    "ink-amber": { family: "sepia", variant: "dark" },
    "mineral-chalk": { family: "graphite", variant: "light" },
    "mineral-slate": { family: "graphite", variant: "dark" },
    "studio-light": { family: "corpus", variant: "light" },
    "studio-dark": { family: "corpus", variant: "dark" }
  };

  const migrated = migrationMap[legacyId] ?? { family: "corpus", variant: "dark" };

  return {
    themeFamilyId: migrated.family,
    themeVariant: migrated.variant,
    fontFamily: raw.fontFamily,
    monoFontFamily: raw.monoFontFamily,
    fontSize: raw.fontSize,
  };
}

export function readAppearancePreference(): AppearancePreference {
  try {
    const raw = localStorage.getItem(APPEARANCE_STORAGE_KEY);
    if (!raw) return { ...DEFAULT_APPEARANCE };
    const parsed = JSON.parse(raw);
    const migrated = migrateLegacyAppearance(parsed);
    return normaliseAppearance(migrated);
  } catch {
    return { ...DEFAULT_APPEARANCE };
  }
}

function saveAppearancePreference(preference: AppearancePreference): void {
  localStorage.setItem(APPEARANCE_STORAGE_KEY, JSON.stringify(preference));
}

export function applyAppearancePreference(preference: AppearancePreference): void {
  const next = normaliseAppearance(preference);
  const root = document.documentElement;
  root.dataset.theme = `${next.themeFamilyId}-${next.themeVariant}`;
  root.style.setProperty("--cw-font-ui", next.fontFamily);
  root.style.setProperty("--cw-font-mono", next.monoFontFamily);
  root.style.setProperty("--cw-font-size-base", `${next.fontSize}px`);
  refreshLightDarkToggleState(next.themeVariant);

  // Dynamically set native titlebar color on Windows
  if (typeof window !== "undefined" && (window as any).__TAURI_INTERNALS__) {
    const titleBarColor = getComputedStyle(root).getPropertyValue("--cw-native-titlebar-color").trim();
    invoke("set_window_titlebar_color", { color: titleBarColor }).catch((e) => {
      console.error("Failed to set titlebar color:", e);
    });
  }
}

function populateSelects(): void {
  dom.appearanceThemeSelect.replaceChildren(...THEME_FAMILIES.map(family => {
    const option = document.createElement("option");
    option.value = family.id;
    option.textContent = family.name;
    return option;
  }));

  dom.appearanceFontSelect.replaceChildren(...FONT_OPTIONS.map(font => {
    const option = document.createElement("option");
    option.value = font.id;
    option.textContent = font.name;
    return option;
  }));

  dom.appearanceMonoFontSelect.replaceChildren(...MONO_FONT_OPTIONS.map(font => {
    const option = document.createElement("option");
    option.value = font.id;
    option.textContent = font.name;
    return option;
  }));
}

function fontIdForStack(stack: string): string {
  return FONT_OPTIONS.find(font => font.stack === stack)?.id ?? FONT_OPTIONS[0].id;
}

function fontStackForId(fontId: string): string {
  return FONT_OPTIONS.find(font => font.id === fontId)?.stack ?? DEFAULT_APPEARANCE.fontFamily;
}

function monoFontIdForStack(stack: string): string {
  return MONO_FONT_OPTIONS.find(font => font.stack === stack)?.id ?? MONO_FONT_OPTIONS[0].id;
}

function monoFontStackForId(fontId: string): string {
  return MONO_FONT_OPTIONS.find(font => font.id === fontId)?.stack ?? DEFAULT_APPEARANCE.monoFontFamily;
}

function syncControls(preference: AppearancePreference): void {
  dom.appearanceThemeSelect.value = preference.themeFamilyId;
  dom.appearanceFontSelect.value = fontIdForStack(preference.fontFamily);
  dom.appearanceMonoFontSelect.value = monoFontIdForStack(preference.monoFontFamily);
  dom.appearanceFontSizeInput.value = String(preference.fontSize);
}

function previewDraft(): void {
  applyAppearancePreference(draftAppearance);
}

function closeAppearanceDialog(revert: boolean): void {
  if (revert) {
    draftAppearance = { ...savedAppearance };
    applyAppearancePreference(savedAppearance);
  }
  dom.appearanceModal.classList.add("hidden");
}

function openAppearanceDialog(): void {
  savedAppearance = readAppearancePreference();
  draftAppearance = { ...savedAppearance };
  syncControls(draftAppearance);
  dom.appearanceModal.classList.remove("hidden");
  dom.appearanceThemeSelect.focus();
}

export function toggleLightDarkTheme(): void {
  const currentAppearance = readAppearancePreference();
  const nextVariant = oppositeMode(currentAppearance.themeVariant);
  const nextAppearance = normaliseAppearance({
    ...currentAppearance,
    themeVariant: nextVariant,
  });
  savedAppearance = nextAppearance;
  draftAppearance = { ...nextAppearance };
  applyAppearancePreference(nextAppearance);
  saveAppearancePreference(nextAppearance);
  syncControls(nextAppearance);
}

export function initAppearanceDialog(): void {
  populateSelects();
  savedAppearance = readAppearancePreference();
  draftAppearance = { ...savedAppearance };
  applyAppearancePreference(savedAppearance);
  syncControls(savedAppearance);

  dom.menuAppearance.addEventListener("click", openAppearanceDialog);
  dom.appearanceCloseButton.addEventListener("click", () => closeAppearanceDialog(true));
  dom.appearanceCancelButton.addEventListener("click", () => closeAppearanceDialog(true));

  dom.appearanceThemeSelect.addEventListener("change", () => {
    draftAppearance = {
      ...draftAppearance,
      themeFamilyId: dom.appearanceThemeSelect.value,
    };
    previewDraft();
  });

  dom.appearanceFontSelect.addEventListener("change", () => {
    draftAppearance = {
      ...draftAppearance,
      fontFamily: fontStackForId(dom.appearanceFontSelect.value),
    };
    previewDraft();
  });

  dom.appearanceMonoFontSelect.addEventListener("change", () => {
    draftAppearance = {
      ...draftAppearance,
      monoFontFamily: monoFontStackForId(dom.appearanceMonoFontSelect.value),
    };
    previewDraft();
  });

  dom.appearanceFontSizeInput.addEventListener("input", () => {
    const fontSize = clampFontSize(dom.appearanceFontSizeInput.valueAsNumber);
    draftAppearance = { ...draftAppearance, fontSize };
    previewDraft();
  });

  dom.appearanceFontSizeInput.addEventListener("change", () => {
    dom.appearanceFontSizeInput.value = String(draftAppearance.fontSize);
  });

  dom.appearanceResetButton.addEventListener("click", () => {
    draftAppearance = { ...DEFAULT_APPEARANCE };
    syncControls(draftAppearance);
    previewDraft();
  });

  dom.appearanceSaveButton.addEventListener("click", () => {
    const nextAppearance = normaliseAppearance(draftAppearance);
    savedAppearance = nextAppearance;
    draftAppearance = { ...nextAppearance };
    applyAppearancePreference(nextAppearance);
    saveAppearancePreference(nextAppearance);
    dom.appearanceModal.classList.add("hidden");
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !dom.appearanceModal.classList.contains("hidden")) {
      closeAppearanceDialog(true);
    }
  });
}
