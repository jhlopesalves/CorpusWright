import { dom } from "./dom.ts";

export const APPEARANCE_STORAGE_KEY = "corpuswright.appearance.v1";

const SYSTEM_UI_STACK = 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif';
const DEFAULT_MONO_STACK = '"JetBrains Mono", "Cascadia Mono", "Segoe UI Mono", Consolas, ui-monospace, monospace';
const DEFAULT_THEME_ID = "dark";
const DEFAULT_FONT_SIZE = 16;
const MIN_FONT_SIZE = 12;
const MAX_FONT_SIZE = 20;

interface ThemePreset {
  id: string;
  name: string;
}

interface FontOption {
  id: string;
  name: string;
  stack: string;
}

interface AppearancePreference {
  themeId: string;
  fontFamily: string;
  monoFontFamily: string;
  fontSize: number;
}

export const THEME_PRESETS: ThemePreset[] = [
  { id: "dark", name: "Corpus Dark" },
  { id: "light", name: "Corpus Light" },
  { id: "repository-light", name: "Repository Light" },
  { id: "repository-dark", name: "Repository Dark" },
  { id: "clear-day", name: "Clear Day" },
  { id: "clear-night", name: "Clear Night" },
  { id: "spectrum-paper", name: "Spectrum Paper" },
  { id: "spectrum-ink", name: "Spectrum Ink" },
  { id: "retro-paper", name: "Retro Paper" },
  { id: "retro-console", name: "Retro Console" },
  { id: "graphite-desk", name: "Graphite Desk" },
  { id: "cloud-desk", name: "Cloud Desk" },
  { id: "cloud-night", name: "Cloud Night" },
  { id: "solar-paper", name: "Solar Paper" },
  { id: "solar-night", name: "Solar Night" },
  { id: "ink-amber", name: "Ink & Amber" },
  { id: "sepia-reading", name: "Sepia Reading" },
  { id: "moss-desk", name: "Moss Desk" },
  { id: "mineral-slate", name: "Mineral Slate" },
  { id: "studio-light", name: "Studio Light" },
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

const DEFAULT_APPEARANCE: AppearancePreference = {
  themeId: DEFAULT_THEME_ID,
  fontFamily: SYSTEM_UI_STACK,
  monoFontFamily: DEFAULT_MONO_STACK,
  fontSize: DEFAULT_FONT_SIZE,
};

let savedAppearance = readAppearancePreference();
let draftAppearance = { ...savedAppearance };

function clampFontSize(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_FONT_SIZE;
  return Math.min(MAX_FONT_SIZE, Math.max(MIN_FONT_SIZE, Math.round(value)));
}

function isKnownTheme(themeId: string): boolean {
  return THEME_PRESETS.some(theme => theme.id === themeId);
}

function isKnownFontStack(fontFamily: string): boolean {
  return FONT_OPTIONS.some(font => font.stack === fontFamily);
}

function isKnownMonoFontStack(fontFamily: string): boolean {
  return MONO_FONT_OPTIONS.some(font => font.stack === fontFamily);
}

function normaliseAppearance(value: Partial<AppearancePreference>): AppearancePreference {
  const themeId = typeof value.themeId === "string" && isKnownTheme(value.themeId)
    ? value.themeId
    : DEFAULT_APPEARANCE.themeId;
  const fontFamily = typeof value.fontFamily === "string" && isKnownFontStack(value.fontFamily)
    ? value.fontFamily
    : DEFAULT_APPEARANCE.fontFamily;
  const monoFontFamily = typeof value.monoFontFamily === "string" && isKnownMonoFontStack(value.monoFontFamily)
    ? value.monoFontFamily
    : DEFAULT_APPEARANCE.monoFontFamily;
  const fontSize = clampFontSize(Number(value.fontSize));

  return { themeId, fontFamily, monoFontFamily, fontSize };
}

export function readAppearancePreference(): AppearancePreference {
  try {
    const raw = localStorage.getItem(APPEARANCE_STORAGE_KEY);
    if (!raw) return { ...DEFAULT_APPEARANCE };
    const parsed = JSON.parse(raw) as Partial<AppearancePreference>;
    return normaliseAppearance(parsed);
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
  root.dataset.theme = next.themeId;
  root.style.setProperty("--cw-font-ui", next.fontFamily);
  root.style.setProperty("--cw-font-mono", next.monoFontFamily);
  root.style.setProperty("--cw-font-size-base", `${next.fontSize}px`);
}

function populateSelects(): void {
  dom.appearanceThemeSelect.replaceChildren(...THEME_PRESETS.map(theme => {
    const option = document.createElement("option");
    option.value = theme.id;
    option.textContent = theme.name;
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
  dom.appearanceThemeSelect.value = preference.themeId;
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
  const currentTheme = document.documentElement.dataset.theme;
  const nextTheme = currentTheme === "light" ? "dark" : "light";
  const nextAppearance = normaliseAppearance({
    ...readAppearancePreference(),
    themeId: nextTheme,
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
      themeId: dom.appearanceThemeSelect.value,
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
