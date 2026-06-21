import { dom } from "./dom.ts";

export const APPEARANCE_STORAGE_KEY = "corpuswright.appearance.v1";

const SYSTEM_UI_STACK = 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif';
const DEFAULT_MONO_STACK = '"JetBrains Mono", "Cascadia Mono", "Segoe UI Mono", Consolas, ui-monospace, monospace';
const DEFAULT_DARK_THEME_ID = "dark";
const DEFAULT_LIGHT_THEME_ID = "light";
const DEFAULT_FONT_SIZE = 16;
const MIN_FONT_SIZE = 12;
const MAX_FONT_SIZE = 20;

export type ThemeMode = "light" | "dark";

export interface ThemePreset {
  id: string;
  name: string;
  mode: ThemeMode;
  pairId?: string;
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
  { id: "dark", name: "Corpus Dark", mode: "dark", pairId: "corpus" },
  { id: "light", name: "Corpus Light", mode: "light", pairId: "corpus" },
  { id: "repository-light", name: "Repository Light", mode: "light", pairId: "repository" },
  { id: "repository-dark", name: "Repository Dark", mode: "dark", pairId: "repository" },
  { id: "clear-day", name: "Clear Day", mode: "light", pairId: "clear" },
  { id: "clear-night", name: "Clear Night", mode: "dark", pairId: "clear" },
  { id: "spectrum-paper", name: "Spectrum Paper", mode: "light", pairId: "spectrum" },
  { id: "spectrum-ink", name: "Spectrum Ink", mode: "dark", pairId: "spectrum" },
  { id: "retro-paper", name: "Retro Paper", mode: "light", pairId: "retro" },
  { id: "retro-console", name: "Retro Console", mode: "dark", pairId: "retro" },
  { id: "graphite-day", name: "Graphite Day", mode: "light", pairId: "graphite" },
  { id: "graphite-desk", name: "Graphite Desk", mode: "dark", pairId: "graphite" },
  { id: "cloud-desk", name: "Cloud Desk", mode: "light", pairId: "cloud" },
  { id: "cloud-night", name: "Cloud Night", mode: "dark", pairId: "cloud" },
  { id: "solar-paper", name: "Solar Paper", mode: "light", pairId: "solar" },
  { id: "solar-night", name: "Solar Night", mode: "dark", pairId: "solar" },
  { id: "amber-paper", name: "Amber Paper", mode: "light", pairId: "amber" },
  { id: "ink-amber", name: "Ink & Amber", mode: "dark", pairId: "amber" },
  { id: "sepia-reading", name: "Sepia Reading", mode: "light" },
  { id: "moss-day", name: "Moss Day", mode: "light", pairId: "moss" },
  { id: "moss-desk", name: "Moss Desk", mode: "dark", pairId: "moss" },
  { id: "mineral-chalk", name: "Mineral Chalk", mode: "light", pairId: "mineral" },
  { id: "mineral-slate", name: "Mineral Slate", mode: "dark", pairId: "mineral" },
  { id: "studio-light", name: "Studio Light", mode: "light", pairId: "studio" },
  { id: "studio-dark", name: "Studio Dark", mode: "dark", pairId: "studio" },
  { id: "high-contrast", name: "High Contrast", mode: "dark" },
];

export const INTENTIONAL_SINGLETONS = ["sepia-reading", "high-contrast"] as const;

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

function preferredDefaultThemeId(): string {
  if (
    typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-color-scheme: light)").matches
  ) {
    return DEFAULT_LIGHT_THEME_ID;
  }
  return DEFAULT_DARK_THEME_ID;
}

function createDefaultAppearance(): AppearancePreference {
  return {
    themeId: preferredDefaultThemeId(),
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

function isKnownTheme(themeId: string): boolean {
  return THEME_PRESETS.some(theme => theme.id === themeId);
}

function findThemePreset(themeId: string): ThemePreset | undefined {
  return THEME_PRESETS.find(theme => theme.id === themeId);
}

function pairedThemeFor(theme: ThemePreset): ThemePreset | undefined {
  if (!theme.pairId) {
    return undefined;
  }
  return THEME_PRESETS.find(candidate => candidate.pairId === theme.pairId && candidate.mode !== theme.mode);
}

function oppositeMode(mode: ThemeMode): ThemeMode {
  return mode === "light" ? "dark" : "light";
}

function activeThemePreset(): ThemePreset | undefined {
  const activeTheme = document.documentElement.dataset.theme;
  if (activeTheme && isKnownTheme(activeTheme)) {
    return findThemePreset(activeTheme);
  }
  return findThemePreset(readAppearancePreference().themeId);
}

function setThemeToggleIcon(targetMode: ThemeMode): void {
  dom.themeToggle.innerHTML = targetMode === "light" ? SUN_ICON : MOON_ICON;
}

function refreshLightDarkToggleState(themeId: string): void {
  const theme = findThemePreset(themeId) ?? findThemePreset(DEFAULT_APPEARANCE.themeId);
  if (!theme) {
    return;
  }

  const pairedTheme = pairedThemeFor(theme);
  const targetMode = pairedTheme?.mode ?? oppositeMode(theme.mode);

  if (!pairedTheme) {
    const unavailable = `No ${targetMode} variant for ${theme.name}`;
    dom.themeToggle.disabled = true;
    dom.themeToggle.title = unavailable;
    dom.themeToggle.setAttribute("aria-label", unavailable);
    dom.menuToggleTheme.textContent = unavailable;
    dom.menuToggleTheme.title = unavailable;
    dom.menuToggleTheme.setAttribute("aria-disabled", "true");
    dom.menuToggleTheme.tabIndex = -1;
    setThemeToggleIcon(targetMode);
    return;
  }

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
  refreshLightDarkToggleState(next.themeId);
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
  const currentTheme = activeThemePreset();
  const nextTheme = currentTheme ? pairedThemeFor(currentTheme) : undefined;
  if (!nextTheme) {
    refreshLightDarkToggleState(currentTheme?.id ?? DEFAULT_APPEARANCE.themeId);
    return;
  }
  const nextAppearance = normaliseAppearance({
    ...readAppearancePreference(),
    themeId: nextTheme.id,
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
