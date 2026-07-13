import {
  attachPlotHover,
  serializeSvgWithResolvedTokens,
} from "./render";
import {
  expandViewBoxForContent,
  wrapSvgText,
  type SvgBounds,
  type SvgViewBox,
} from "./figure-text";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";

import { dom } from "../dom";
import {
  figurePaletteLabel,
  type FigurePaletteMode,
  type OverviewFigureControllerContract,
} from "./figure-workbench";

const FIGURE_COLLAPSE_STORAGE_KEY = "corpuswright.overview.figureCollapse";
const SVG_NS = "http://www.w3.org/2000/svg";
const FIGURE_IDENTITY_HORIZONTAL_PADDING = 12;
const FIGURE_IDENTITY_PLOT_GAP = 14;
const FIGURE_IDENTITY_BLOCK_GAP = 6;
const FIGURE_IDENTITY_BOUNDS_PADDING = 8;

const figureMetadata = new WeakMap<HTMLElement, OverviewFigureOptions>();

export interface OverviewFigureOptions {
  id: string;
  title: string;
  subtitle: string;
  captionStats: string;
  captionDetail?: string;
  captionReading: string;
  controls?: HTMLElement;
  explanation?: HTMLElement;
  controller?: OverviewFigureControllerContract;
  onPinChange?: (key: string, pinned: boolean) => void;
  onClearPins?: () => void;
  exportMetadata?: () => string[];
  layoutFamily?: "standard" | "profile" | "plane" | "roster";
  renderSvg: () => SVGSVGElement;
}

export interface SegmentedControlOption<TValue extends string> {
  value: TValue;
  label: string;
}

export interface OverviewFigureTextUpdate {
  subtitle?: string;
  captionStats?: string;
  captionReading?: string;
}

export function createOverviewFigure(options: OverviewFigureOptions): HTMLElement {
  const figure = document.createElement("figure");
  figure.className = "cw-overview-figure";
  figure.dataset.figureId = options.id;
  figure.dataset.layoutFamily = options.layoutFamily ?? "standard";
  figure.setAttribute("role", "region");
  figure.setAttribute("aria-label", `${options.title}. ${options.subtitle}`);
  figureMetadata.set(figure, options);
  const initialSvg = options.renderSvg();

  const header = document.createElement("div");
  header.className = "cw-overview-figure-header";

  const titleBlock = document.createElement("div");
  titleBlock.className = "cw-overview-figure-title-block";
  titleBlock.setAttribute("aria-label", `${options.title}. ${options.subtitle}`);
  const title = document.createElement("h4");
  title.textContent = options.title;
  const subtitle = document.createElement("p");
  subtitle.className = "cw-overview-figure-subtitle";
  subtitle.textContent = options.subtitle;
  const stats = document.createElement("p");
  stats.className = "cw-overview-figure-stats";
  appendCaptionStats(stats, options.captionStats);
  titleBlock.append(title, subtitle, stats);

  const actions = document.createElement("div");
  actions.className = "cw-overview-figure-actions";

  const exportButton = document.createElement("button");
  exportButton.type = "button";
  exportButton.className = "cw-overview-figure-action cw-overview-figure-export";
  exportButton.title = "Export SVG";
  exportButton.setAttribute("aria-label", `Export ${options.title} as SVG`);
  exportButton.textContent = "Export SVG";

  const collapseButton = document.createElement("button");
  collapseButton.type = "button";
  collapseButton.className = "cw-overview-figure-action cw-overview-figure-collapse";
  collapseButton.title = "Collapse figure";
  collapseButton.setAttribute("aria-label", `Collapse ${options.title}`);

  const collapseLabel = document.createElement("span");
  collapseLabel.textContent = "Collapse";
  collapseButton.appendChild(collapseLabel);
  const clearPinsButton = document.createElement("button");
  clearPinsButton.type = "button";
  clearPinsButton.className = "cw-overview-figure-action cw-overview-figure-clear-pins";
  clearPinsButton.textContent = "Clear pins";
  clearPinsButton.hidden = !(options.controller?.pins.size);
  actions.append(clearPinsButton, exportButton, collapseButton);
  header.append(titleBlock, actions);

  const body = document.createElement("div");
  body.className = "cw-overview-figure-body";
  body.dataset.figureSvgMount = "true";
  replaceFigureSvgElement(body, initialSvg);
  clearPinsButton.addEventListener("click", () => {
    options.controller?.clearPins();
    replaceFigureSvgElement(body, options.renderSvg());
    syncClearPinsAction(clearPinsButton, options.controller);
    options.onClearPins?.();
  });

  const caption = document.createElement("figcaption");
  caption.className = "cw-overview-figure-caption";
  const detail = options.captionDetail ? document.createElement("p") : null;
  if (detail) {
    detail.className = "cw-overview-figure-caption-detail";
    detail.textContent = options.captionDetail ?? "";
  }
  const reading = document.createElement("p");
  reading.className = "cw-overview-figure-caption-reading";
  reading.textContent = options.captionReading;
  if (detail) caption.append(detail);
  caption.append(reading);

  const explanation = options.explanation;
  if (explanation) explanation.classList.add("cw-overview-figure-explanation");

  const collapsed = readCollapsedFigures().has(options.id);
  setFigureCollapsed(figure, collapseButton, collapseLabel, options.title, collapsed);

  collapseButton.addEventListener("click", () => {
    const next = !figure.classList.contains("is-collapsed");
    const collapsedFigures = readCollapsedFigures();
    if (next) collapsedFigures.add(options.id);
    else collapsedFigures.delete(options.id);
    writeCollapsedFigures(collapsedFigures);
    setFigureCollapsed(figure, collapseButton, collapseLabel, options.title, next);
  });

  const exportDialog = createExportDialog(figure, options.id);
  exportButton.addEventListener("click", () => exportDialog.showModal());

  figure.addEventListener("cw-plot-pin-toggle", (event) => {
    if (!(event instanceof CustomEvent) || typeof event.detail?.key !== "string") return;
    const controller = options.controller;
    if (!controller) return;
    const pinned = controller.togglePin(event.detail.key);
    replaceFigureSvgElement(body, options.renderSvg());
    syncClearPinsAction(clearPinsButton, controller);
    options.onPinChange?.(event.detail.key, pinned);
  });

  figure.appendChild(header);
  if (options.controls) {
    options.controls.classList.add("cw-overview-figure-controls");
    const controlRow = document.createElement("div");
    controlRow.className = "cw-overview-figure-control-row";
    controlRow.appendChild(options.controls);
    figure.appendChild(controlRow);
  }
  figure.append(body, caption);
  if (explanation) figure.appendChild(explanation);
  figure.appendChild(exportDialog);
  return figure;
}

export function replaceFigureSvg(
  figure: HTMLElement,
  svg: SVGSVGElement,
  textUpdate?: OverviewFigureTextUpdate,
): void {
  const mount = figure.querySelector<HTMLElement>("[data-figure-svg-mount='true']");
  if (!mount) return;
  const previous = figureMetadata.get(figure);
  const options = previous && textUpdate ? { ...previous, ...textUpdate } : previous;
  if (options && textUpdate) {
    figureMetadata.set(figure, options);
    refreshFigureText(figure, options);
  }
  replaceFigureSvgElement(mount, svg);
  const clearPins = figure.querySelector<HTMLButtonElement>(".cw-overview-figure-clear-pins");
  if (clearPins) syncClearPinsAction(clearPins, options?.controller);
}

function refreshFigureText(figure: HTMLElement, options: OverviewFigureOptions): void {
  figure.setAttribute("aria-label", `${options.title}. ${options.subtitle}`);
  const titleBlock = figure.querySelector<HTMLElement>(".cw-overview-figure-title-block");
  if (titleBlock) titleBlock.setAttribute("aria-label", `${options.title}. ${options.subtitle}`);
  const subtitle = figure.querySelector<HTMLElement>(".cw-overview-figure-subtitle");
  if (subtitle) subtitle.textContent = options.subtitle;
  const stats = figure.querySelector<HTMLElement>(".cw-overview-figure-stats");
  if (stats) {
    stats.replaceChildren();
    appendCaptionStats(stats, options.captionStats);
  }
  const reading = figure.querySelector<HTMLElement>(".cw-overview-figure-caption-reading:last-child");
  if (reading) reading.textContent = options.captionReading;
}

export function createOverviewSegmentedControl<TValue extends string>(
  label: string,
  options: Array<SegmentedControlOption<TValue>>,
  initialValue: TValue,
  onChange: (value: TValue) => void,
): HTMLElement {
  const field = document.createElement("div");
  field.className = "cw-overview-control-field cw-overview-segmented-field";
  field.appendChild(createControlLabel(label));
  const group = document.createElement("div");
  group.className = "cw-overview-segmented-control";
  group.setAttribute("role", "group");
  group.setAttribute("aria-label", label);
  for (const option of options) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "cw-overview-segment";
    button.textContent = option.label;
    button.dataset.value = option.value;
    button.setAttribute("aria-pressed", String(option.value === initialValue));
    button.addEventListener("click", () => {
      for (const sibling of group.querySelectorAll<HTMLButtonElement>(".cw-overview-segment")) {
        sibling.setAttribute("aria-pressed", String(sibling === button));
      }
      onChange(option.value);
    });
    group.appendChild(button);
  }
  field.appendChild(group);
  return field;
}

export function createOverviewSelectControl<TValue extends string>(
  label: string,
  options: Array<SegmentedControlOption<TValue>>,
  initialValue: TValue,
  onChange: (value: TValue) => void,
): HTMLElement {
  const field = createControlField(label);
  const select = document.createElement("select");
  select.className = "cw-overview-control-select";
  select.setAttribute("aria-label", label);
  for (const option of options) {
    const element = document.createElement("option");
    element.value = option.value;
    element.textContent = option.label;
    element.selected = option.value === initialValue;
    select.appendChild(element);
  }
  select.addEventListener("change", () => onChange(select.value as TValue));
  field.appendChild(select);
  return field;
}

export function createOverviewRangeControl(
  label: string,
  options: {
    minimum: number;
    maximum: number;
    step: number;
    initialValue: number;
    onInput?: (value: number) => void;
    onChange: (value: number) => void;
  },
): HTMLElement {
  const field = createControlField(label);
  field.classList.add("cw-overview-range-control");
  const range = document.createElement("input");
  range.type = "range";
  range.min = String(options.minimum);
  range.max = String(options.maximum);
  range.step = String(options.step);
  range.value = String(options.initialValue);
  range.setAttribute("aria-label", `${label} slider`);
  const number = document.createElement("input");
  number.type = "number";
  number.min = range.min;
  number.max = range.max;
  number.step = range.step;
  number.value = range.value;
  number.className = "cw-overview-control-number";
  number.setAttribute("aria-label", `${label} value`);
  const clamp = (value: number) => Math.min(options.maximum, Math.max(options.minimum, value));
  range.addEventListener("input", () => {
    number.value = range.value;
    options.onInput?.(Number(range.value));
  });
  range.addEventListener("change", () => options.onChange(Number(range.value)));
  number.addEventListener("change", () => {
    const value = clamp(Number(number.value));
    number.value = String(value);
    range.value = String(value);
    options.onChange(value);
  });
  field.append(range, number);
  return field;
}

export function createOverviewToggleControl(
  label: string,
  initialValue: boolean,
  onChange: (value: boolean) => void,
): HTMLElement {
  const field = document.createElement("div");
  field.className = "cw-overview-control-field cw-overview-toggle-field";
  field.appendChild(createControlLabel(label));
  const button = document.createElement("button");
  button.type = "button";
  button.className = "cw-overview-toggle-control";
  let value = initialValue;
  const refresh = () => {
    button.textContent = value ? "Shown" : "Hidden";
    button.setAttribute("aria-label", `${label}: ${value ? "shown" : "hidden"}`);
    button.setAttribute("aria-pressed", String(value));
  };
  refresh();
  button.addEventListener("click", () => {
    value = !value;
    refresh();
    onChange(value);
  });
  field.appendChild(button);
  return field;
}

export function createOverviewResetAction(label: string, onReset: () => void): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "cw-overview-figure-action cw-overview-reset-action";
  button.textContent = label;
  button.addEventListener("click", onReset);
  return button;
}

export function overviewFigureSvgString(
  figure: HTMLElement,
  paletteMode: FigurePaletteMode = "theme",
): string | null {
  const svg = figure.querySelector<SVGSVGElement>(".cw-plot-svg");
  const options = figureMetadata.get(figure);
  if (!svg || !options) return null;
  const exportedSvg = svg.cloneNode(true) as SVGSVGElement;
  prepareStaticExportClone(exportedSvg);
  const exportMount = document.createElement("div");
  exportMount.className = "cw-plot-export-mount";
  exportMount.dataset.figurePalette = paletteMode;
  exportMount.style.setProperty("--cw-plot-export-width", `${svg.viewBox.baseVal.width || 720}px`);
  exportMount.appendChild(exportedSvg);
  figure.appendChild(exportMount);
  try {
    decorateFigureSvg(exportedSvg, exportOptions(options, paletteMode));
    return serializeSvgWithResolvedTokens(exportedSvg);
  } finally {
    exportMount.remove();
  }
}

function exportOptions(
  options: OverviewFigureOptions,
  paletteMode: FigurePaletteMode,
): OverviewFigureOptions {
  const metadata = [
    ...(options.controller?.exportMetadata() ?? []),
    ...(options.exportMetadata?.() ?? []),
  ];
  if (paletteMode !== "theme") {
    metadata.push(`palette ${figurePaletteLabel(paletteMode)}`);
  }
  return metadata.length === 0
    ? options
    : { ...options, captionStats: `${options.captionStats} · ${metadata.join(" · ")}` };
}

function replaceFigureSvgElement(mount: HTMLElement, svg: SVGSVGElement): void {
  mount.replaceChildren(svg);
  attachPlotHover(svg);
}

function setFigureCollapsed(
  figure: HTMLElement,
  button: HTMLButtonElement,
  label: HTMLElement,
  title: string,
  collapsed: boolean,
): void {
  figure.classList.toggle("is-collapsed", collapsed);
  button.setAttribute("aria-expanded", String(!collapsed));
  button.title = collapsed ? "Expand figure" : "Collapse figure";
  button.setAttribute("aria-label", collapsed ? `Expand ${title}` : `Collapse ${title}`);
  label.textContent = collapsed ? "Expand" : "Collapse";
}

function syncClearPinsAction(
  button: HTMLButtonElement,
  controller: OverviewFigureControllerContract | undefined,
): void {
  button.hidden = !controller || controller.pins.size === 0;
}

function createControlLabel(label: string): HTMLSpanElement {
  const element = document.createElement("span");
  element.className = "cw-overview-control-label";
  element.textContent = label;
  return element;
}

function createControlField(label: string): HTMLElement {
  const field = document.createElement("label");
  field.className = "cw-overview-control-field";
  field.appendChild(createControlLabel(label));
  return field;
}

function decorateFigureSvg(svg: SVGSVGElement, options: OverviewFigureOptions): void {
  const viewBox = svg.viewBox.baseVal;
  const plot: SvgViewBox = {
    x: viewBox.x,
    y: viewBox.y,
    width: viewBox.width || 720,
    height: viewBox.height || 360,
  };
  const accessibleId = options.id.replace(/[^a-z0-9_-]+/gi, "-");
  const titleId = `${accessibleId}-export-title`;
  const descriptionId = `${accessibleId}-export-description`;
  const accessibleTitle = document.createElementNS(SVG_NS, "title");
  accessibleTitle.id = titleId;
  accessibleTitle.textContent = options.title;
  const accessibleDescription = document.createElementNS(SVG_NS, "desc");
  accessibleDescription.id = descriptionId;
  accessibleDescription.textContent = [
    options.title,
    options.subtitle,
    options.captionStats,
    options.captionDetail,
    options.captionReading,
  ].filter(Boolean).join(". ");
  svg.prepend(accessibleTitle, accessibleDescription);
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-labelledby", `${titleId} ${descriptionId}`);
  svg.removeAttribute("aria-label");

  const identity = document.createElementNS(SVG_NS, "g");
  identity.setAttribute("class", "cw-plot-figure-identity");
  identity.setAttribute("aria-hidden", "true");
  const title = createSvgText("cw-plot-figure-title");
  const subtitle = createSvgText("cw-plot-figure-subtitle");
  const stats = createSvgText("cw-plot-figure-stats");
  const detail = options.captionDetail
    ? createSvgText("cw-plot-figure-detail")
    : null;
  const reading = createSvgText("cw-plot-figure-reading");
  identity.append(title, subtitle, stats);
  if (detail) identity.append(detail);
  identity.append(reading);
  svg.appendChild(identity);

  const textX = plot.x + FIGURE_IDENTITY_HORIZONTAL_PADDING;
  const maxTextWidth = plot.width - (FIGURE_IDENTITY_HORIZONTAL_PADDING * 2);
  const titleLayout = prepareTextLayout(title, options.title, maxTextWidth);
  const subtitleLayout = prepareTextLayout(subtitle, options.subtitle, maxTextWidth);
  const statsLayout = prepareTextLayout(stats, options.captionStats, maxTextWidth);
  const detailLayout = detail && options.captionDetail
    ? prepareTextLayout(detail, options.captionDetail, maxTextWidth)
    : null;
  const readingLayout = prepareTextLayout(reading, options.captionReading, maxTextWidth);

  const topLayouts = [titleLayout, subtitleLayout, statsLayout];
  const topHeight = totalTextHeight(topLayouts);
  let topY = plot.y - FIGURE_IDENTITY_PLOT_GAP - topHeight;
  const fallbackBounds: SvgBounds[] = [];
  for (const layout of topLayouts) {
    fallbackBounds.push(positionTextLayout(layout, textX, topY + layout.fontSize));
    topY += textLayoutHeight(layout) + FIGURE_IDENTITY_BLOCK_GAP;
  }

  const bottomLayouts = [detailLayout, readingLayout].filter(
    (layout): layout is PreparedTextLayout => layout !== null,
  );
  let bottomY = plot.y + plot.height + FIGURE_IDENTITY_PLOT_GAP;
  for (const layout of bottomLayouts) {
    fallbackBounds.push(positionTextLayout(layout, textX, bottomY + layout.fontSize));
    bottomY += textLayoutHeight(layout) + FIGURE_IDENTITY_BLOCK_GAP;
  }

  const measuredBounds = [title, subtitle, stats, ...(detail ? [detail] : []), reading]
    .map((element, index) => svgTextBounds(element) ?? fallbackBounds[index]!);
  const expanded = expandViewBoxForContent(plot, measuredBounds, FIGURE_IDENTITY_BOUNDS_PADDING);
  svg.setAttribute("viewBox", `${expanded.x} ${expanded.y} ${expanded.width} ${expanded.height}`);
  const background = document.createElementNS(SVG_NS, "rect");
  background.setAttribute("class", "cw-plot-export-background");
  background.setAttribute("x", String(expanded.x));
  background.setAttribute("y", String(expanded.y));
  background.setAttribute("width", String(expanded.width));
  background.setAttribute("height", String(expanded.height));
  svg.insertBefore(background, accessibleDescription.nextSibling);
}

function prepareStaticExportClone(svg: SVGSVGElement): void {
  svg.classList.remove("is-hovering");
  svg.classList.add("cw-plot-export-static");
  svg.removeAttribute("tabindex");
  delete svg.dataset.plotNavigationReady;
  for (const transient of svg.querySelectorAll(".cw-plot-crosshair-x, .cw-plot-crosshair-y, .cw-plot-tooltip")) {
    transient.remove();
  }
  for (const target of svg.querySelectorAll<SVGElement>(".cw-plot-hover-target")) {
    target.classList.remove("cw-plot-hover-target");
    target.removeAttribute("tabindex");
    target.removeAttribute("role");
    target.removeAttribute("aria-pressed");
    target.removeAttribute("data-plot-x");
    target.removeAttribute("data-plot-y");
    target.removeAttribute("data-tooltip-title");
    target.removeAttribute("data-tooltip-rows");
    target.removeAttribute("data-pin-key");
  }
  for (const element of [svg, ...Array.from(svg.querySelectorAll<SVGElement>("*"))]) {
    const inlineStyle = element.getAttribute("style");
    if (inlineStyle === null) continue;
    const staticStyle = inlineStyle.split(";")
      .map(declaration => declaration.trim())
      .filter(Boolean)
      .filter(declaration => !/^(?:--cw-plot-stagger|animation|animation-delay|transition)\s*:/.test(declaration))
      .join("; ");
    if (staticStyle) element.setAttribute("style", `${staticStyle};`);
    else element.removeAttribute("style");
  }
}

interface PreparedTextLayout {
  element: SVGTextElement;
  lines: string[];
  widths: number[];
  fontSize: number;
  lineHeight: number;
}

function createSvgText(className: string): SVGTextElement {
  const element = document.createElementNS(SVG_NS, "text");
  element.setAttribute("class", className);
  return element;
}

function prepareTextLayout(
  element: SVGTextElement,
  value: string,
  maxWidth: number,
): PreparedTextLayout {
  const style = window.getComputedStyle(element);
  const fontSize = cssPixelValue(style.fontSize, 12);
  const lineHeight = cssPixelValue(style.lineHeight, fontSize * 1.35);
  const measure = (candidate: string): number => {
    element.textContent = candidate;
    return element.getComputedTextLength();
  };
  const lines = wrapSvgText(value, maxWidth, measure);
  const widths = lines.map(measure);
  element.replaceChildren();
  return { element, lines, widths, fontSize, lineHeight };
}

function positionTextLayout(
  layout: PreparedTextLayout,
  x: number,
  firstBaseline: number,
): SvgBounds {
  layout.element.setAttribute("x", String(x));
  layout.element.setAttribute("y", String(firstBaseline));
  layout.lines.forEach((line, index) => {
    const tspan = document.createElementNS(SVG_NS, "tspan");
    tspan.setAttribute("x", String(x));
    tspan.setAttribute("y", String(firstBaseline + (index * layout.lineHeight)));
    tspan.textContent = line;
    layout.element.appendChild(tspan);
  });
  return {
    x,
    y: firstBaseline - layout.fontSize,
    width: Math.max(0, ...layout.widths),
    height: textLayoutHeight(layout),
  };
}

function totalTextHeight(layouts: PreparedTextLayout[]): number {
  return layouts.reduce((total, layout) => total + textLayoutHeight(layout), 0)
    + (Math.max(0, layouts.length - 1) * FIGURE_IDENTITY_BLOCK_GAP);
}

function textLayoutHeight(layout: PreparedTextLayout): number {
  if (layout.lines.length === 0) return 0;
  return layout.fontSize + ((layout.lines.length - 1) * layout.lineHeight);
}

function svgTextBounds(element: SVGTextElement): SvgBounds | null {
  try {
    const bounds = element.getBBox();
    if ([bounds.x, bounds.y, bounds.width, bounds.height].every(Number.isFinite)) {
      return { x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height };
    }
  } catch {
    // The logical line layout remains a deterministic fallback when SVG bounds are unavailable.
  }
  return null;
}

function cssPixelValue(value: string, fallback: number): number {
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function appendCaptionStats(container: HTMLElement, value: string): void {
  const numericPattern = /(\(?[~>=]*\d[\d,]*(?:\.\d+)?(?:-\d[\d,]*(?:\.\d+)?)?(?:%|x)?\)?)/g;
  let lastIndex = 0;
  for (const match of value.matchAll(numericPattern)) {
    const index = match.index ?? 0;
    if (index > lastIndex) {
      container.appendChild(createCaptionStatSpan(value.slice(lastIndex, index), "cw-overview-figure-caption-metric"));
    }
    container.appendChild(createCaptionStatSpan(match[0], "cw-overview-figure-caption-value"));
    lastIndex = index + match[0].length;
  }
  if (lastIndex < value.length) {
    container.appendChild(createCaptionStatSpan(value.slice(lastIndex), "cw-overview-figure-caption-metric"));
  }
}

function createCaptionStatSpan(text: string, className: string): HTMLElement {
  const span = document.createElement("span");
  span.className = className;
  span.textContent = text;
  return span;
}

function readCollapsedFigures(): Set<string> {
  try {
    const raw = localStorage.getItem(FIGURE_COLLAPSE_STORAGE_KEY);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw) as unknown;
    return Array.isArray(parsed)
      ? new Set(parsed.filter((value): value is string => typeof value === "string"))
      : new Set();
  } catch {
    return new Set();
  }
}

function writeCollapsedFigures(values: Set<string>): void {
  try {
    localStorage.setItem(FIGURE_COLLAPSE_STORAGE_KEY, JSON.stringify([...values].sort()));
  } catch {
    // Persistence is best-effort; the figure still works without storage.
  }
}

function createExportDialog(figure: HTMLElement, figureId: string): HTMLDialogElement {
  const dialog = document.createElement("dialog");
  dialog.className = "cw-overview-export-dialog";
  dialog.setAttribute("aria-labelledby", `${figureId}-export-heading`);
  const form = document.createElement("form");
  form.method = "dialog";
  const heading = document.createElement("h5");
  heading.id = `${figureId}-export-heading`;
  heading.textContent = "Export SVG colours";
  const choices = document.createElement("fieldset");
  const legend = document.createElement("legend");
  legend.textContent = "Colour mode";
  choices.appendChild(legend);
  const modes: Array<{ value: FigurePaletteMode; label: string }> = [
    { value: "theme", label: "Current theme colours" },
    { value: "colorBlind", label: "Colour-blind-safe colours" },
    { value: "monochrome", label: "Monochrome / print" },
  ];
  for (const [index, mode] of modes.entries()) {
    const choice = document.createElement("label");
    const input = document.createElement("input");
    input.type = "radio";
    input.name = `${figureId}-export-palette`;
    input.value = mode.value;
    input.checked = index === 0;
    choice.append(input, document.createTextNode(mode.label));
    choices.appendChild(choice);
  }
  const actions = document.createElement("div");
  actions.className = "cw-overview-export-dialog-actions";
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.textContent = "Cancel";
  cancel.addEventListener("click", () => dialog.close());
  const download = document.createElement("button");
  download.type = "button";
  download.textContent = "Download SVG";
  download.addEventListener("click", () => {
    const selected = form.querySelector<HTMLInputElement>("input[type='radio']:checked");
    const paletteMode = (selected?.value ?? "theme") as FigurePaletteMode;
    dialog.close();
    void saveFigureSvg(figure, figureId, paletteMode);
  });
  actions.append(cancel, download);
  form.append(heading, choices, actions);
  dialog.appendChild(form);
  return dialog;
}

async function saveFigureSvg(
  figure: HTMLElement,
  figureId: string,
  paletteMode: FigurePaletteMode,
): Promise<void> {
  const svg = figure.querySelector<SVGSVGElement>(".cw-plot-svg");
  if (!svg) return;
  const path = await save({
    defaultPath: `${figureId}.svg`,
    filters: [{ name: "SVG", extensions: ["svg"] }],
  });
  if (path === null) return;
  try {
    const content = overviewFigureSvgString(figure, paletteMode);
    if (content === null) return;
    await invoke("save_svg_file_command", {
      path,
      content,
    });
    dom.statusBar.textContent = `Saved SVG: ${path}`;
  } catch (error) {
    dom.statusBar.textContent = `SVG export error: ${error}`;
    console.error("SVG export error", error);
  }
}
