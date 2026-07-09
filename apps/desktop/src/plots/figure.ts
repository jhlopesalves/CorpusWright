import {
  attachPlotHover,
  serializeSvgWithResolvedTokens,
} from "./render";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";

import { dom } from "../dom";

const FIGURE_COLLAPSE_STORAGE_KEY = "corpuswright.overview.figureCollapse";
const SVG_NS = "http://www.w3.org/2000/svg";
const FIGURE_SVG_TOP_INSET = 52;
const FIGURE_SVG_BOTTOM_INSET = 62;
const FIGURE_TITLE_FONT_SIZE = 17;
const FIGURE_SUBTITLE_FONT_SIZE = 12.5;
const FIGURE_STATS_FONT_SIZE = 12;
const FIGURE_READING_FONT_SIZE = 11.5;

const figureMetadata = new WeakMap<HTMLElement, OverviewFigureOptions>();

export interface OverviewFigureOptions {
  id: string;
  title: string;
  subtitle: string;
  captionStats: string;
  captionDetail?: string;
  captionReading: string;
  controls?: HTMLElement;
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
  const figure = document.createElement("section");
  figure.className = "cw-overview-figure";
  figure.dataset.figureId = options.id;
  figure.setAttribute("role", "region");
  figure.setAttribute("aria-label", `${options.title}. ${options.subtitle}`);
  figureMetadata.set(figure, options);
  const initialSvg = options.renderSvg();

  const header = document.createElement("div");
  header.className = "cw-overview-figure-header";

  const sparkline = document.createElement("span");
  sparkline.className = "cw-overview-figure-sparkline";
  sparkline.dataset.figureSparkline = "true";
  sparkline.setAttribute("aria-hidden", "true");
  renderSparklineGlyph(sparkline, initialSvg);

  const titleBlock = document.createElement("div");
  titleBlock.className = "cw-overview-figure-title-block";
  titleBlock.setAttribute("aria-label", `${options.title}. ${options.subtitle}`);
  const title = document.createElement("h4");
  title.textContent = options.title;
  const subtitle = document.createElement("p");
  subtitle.className = "cw-overview-figure-subtitle";
  subtitle.textContent = options.subtitle;
  titleBlock.append(title, subtitle);

  const actions = document.createElement("div");
  actions.className = "cw-overview-figure-actions";
  if (options.controls) {
    options.controls.classList.add("cw-overview-figure-controls");
    actions.appendChild(options.controls);
  }

  const badgeSlot = document.createElement("span");
  badgeSlot.className = "cw-overview-figure-badge-slot";
  badgeSlot.setAttribute("aria-hidden", "true");

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
  actions.append(badgeSlot, exportButton, collapseButton);
  header.append(sparkline, titleBlock, actions);

  const body = document.createElement("div");
  body.className = "cw-overview-figure-body";
  body.dataset.figureSvgMount = "true";
  replaceFigureSvgElement(body, initialSvg);

  const caption = document.createElement("figcaption");
  caption.className = "cw-overview-figure-caption";
  const stats = document.createElement("p");
  stats.className = "cw-overview-figure-caption-stats";
  appendCaptionStats(stats, options.captionStats);
  const detail = options.captionDetail ? document.createElement("p") : null;
  if (detail) {
    detail.className = "cw-overview-figure-caption-reading";
    detail.textContent = options.captionDetail ?? "";
  }
  const reading = document.createElement("p");
  reading.className = "cw-overview-figure-caption-reading";
  reading.textContent = options.captionReading;
  caption.append(stats);
  if (detail) caption.append(detail);
  caption.append(reading);

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

  exportButton.addEventListener("click", () => void saveFigureSvg(figure, options.id));

  figure.append(header, body, caption);
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
  if (options) decorateFigureSvg(svg, options);
  replaceFigureSvgElement(mount, svg);
  const sparkline = figure.querySelector<HTMLElement>("[data-figure-sparkline='true']");
  if (sparkline) renderSparklineGlyph(sparkline, svg);
}

function refreshFigureText(figure: HTMLElement, options: OverviewFigureOptions): void {
  figure.setAttribute("aria-label", `${options.title}. ${options.subtitle}`);
  const titleBlock = figure.querySelector<HTMLElement>(".cw-overview-figure-title-block");
  if (titleBlock) titleBlock.setAttribute("aria-label", `${options.title}. ${options.subtitle}`);
  const subtitle = figure.querySelector<HTMLElement>(".cw-overview-figure-subtitle");
  if (subtitle) subtitle.textContent = options.subtitle;
  const stats = figure.querySelector<HTMLElement>(".cw-overview-figure-caption-stats");
  if (stats) {
    stats.replaceChildren();
    appendCaptionStats(stats, options.captionStats);
  }
  const reading = figure.querySelector<HTMLElement>(".cw-overview-figure-caption-reading:last-child");
  if (reading) reading.textContent = options.captionReading;
}

export function createSegmentedControl<TValue extends string>(
  label: string,
  options: Array<SegmentedControlOption<TValue>>,
  initialValue: TValue,
  onChange: (value: TValue) => void,
): HTMLElement {
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
  return group;
}

export function overviewFigureSvgString(figure: HTMLElement): string | null {
  const svg = figure.querySelector<SVGSVGElement>(".cw-plot-svg");
  return svg ? serializeSvgWithResolvedTokens(svg) : null;
}

function replaceFigureSvgElement(mount: HTMLElement, svg: SVGSVGElement): void {
  const figure = mount.closest<HTMLElement>(".cw-overview-figure");
  const options = figure ? figureMetadata.get(figure) : undefined;
  if (options && !svg.querySelector(".cw-plot-figure-identity")) decorateFigureSvg(svg, options);
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

function decorateFigureSvg(svg: SVGSVGElement, options: OverviewFigureOptions): void {
  const viewBox = svg.viewBox.baseVal;
  const width = viewBox.width || 720;
  const height = viewBox.height || 360;
  svg.setAttribute("viewBox", `0 ${-FIGURE_SVG_TOP_INSET} ${width} ${height + FIGURE_SVG_TOP_INSET + FIGURE_SVG_BOTTOM_INSET}`);
  svg.setAttribute("aria-label", `${options.title}. ${options.subtitle}. ${options.captionStats}. ${options.captionReading}`);

  const identity = document.createElementNS(SVG_NS, "g");
  identity.setAttribute("class", "cw-plot-figure-identity");
  identity.setAttribute("aria-hidden", "true");

  const title = createSvgText(0, -30, "cw-plot-figure-title", options.title);
  title.setAttribute("font-size", String(FIGURE_TITLE_FONT_SIZE));
  title.setAttribute("font-weight", "650");
  const subtitle = createSvgText(0, -11, "cw-plot-figure-subtitle", options.subtitle);
  subtitle.setAttribute("font-size", String(FIGURE_SUBTITLE_FONT_SIZE));
  const stats = createSvgText(0, height + 24, "cw-plot-figure-stats", options.captionStats);
  stats.setAttribute("font-size", String(FIGURE_STATS_FONT_SIZE));
  const reading = createSvgText(0, height + 45, "cw-plot-figure-reading", options.captionReading);
  reading.setAttribute("font-size", String(FIGURE_READING_FONT_SIZE));
  identity.append(title, subtitle, stats, reading);
  svg.appendChild(identity);
}

function createSvgText(x: number, y: number, className: string, text: string): SVGTextElement {
  const element = document.createElementNS(SVG_NS, "text");
  element.setAttribute("x", String(x));
  element.setAttribute("y", String(y));
  element.setAttribute("class", className);
  element.textContent = text;
  return element;
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

function renderSparklineGlyph(container: HTMLElement, sourceSvg: SVGSVGElement): void {
  const points = Array.from(sourceSvg.querySelectorAll<SVGCircleElement>(".cw-plot-point"))
    .map((point) => ({
      x: Number(point.dataset.plotX ?? point.getAttribute("cx")),
      y: Number(point.dataset.plotY ?? point.getAttribute("cy")),
    }))
    .filter((point) => Number.isFinite(point.x) && Number.isFinite(point.y));
  const sampled = sampleSparklinePoints(points);
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", "0 0 28 14");
  svg.setAttribute("width", "28");
  svg.setAttribute("height", "14");
  svg.setAttribute("focusable", "false");
  svg.classList.add("cw-overview-figure-sparkline-svg");

  const polyline = document.createElementNS(SVG_NS, "polyline");
  polyline.setAttribute("points", sampled.map((point) => `${point.x.toFixed(2)},${point.y.toFixed(2)}`).join(" "));
  polyline.setAttribute("class", "cw-overview-figure-sparkline-line");
  svg.appendChild(polyline);

  sampled.forEach((point, index) => {
    const dot = document.createElementNS(SVG_NS, "circle");
    dot.setAttribute("cx", point.x.toFixed(2));
    dot.setAttribute("cy", point.y.toFixed(2));
    dot.setAttribute("r", "1.7");
    dot.setAttribute("class", `cw-overview-figure-sparkline-dot cw-overview-figure-sparkline-dot-${index}`);
    svg.appendChild(dot);
  });
  container.replaceChildren(svg);
}

function sampleSparklinePoints(points: Array<{ x: number; y: number }>): Array<{ x: number; y: number }> {
  if (points.length === 0) {
    return [
      { x: 2, y: 10 },
      { x: 14, y: 6 },
      { x: 26, y: 4 },
    ];
  }
  const raw = [
    points[0]!,
    points[Math.floor((points.length - 1) / 2)]!,
    points[points.length - 1]!,
  ];
  const minX = Math.min(...raw.map((point) => point.x));
  const maxX = Math.max(...raw.map((point) => point.x));
  const minY = Math.min(...raw.map((point) => point.y));
  const maxY = Math.max(...raw.map((point) => point.y));
  const spanX = maxX - minX || 1;
  const spanY = maxY - minY || 1;
  return raw.map((point) => ({
    x: 2 + ((point.x - minX) / spanX) * 24,
    y: 2 + ((point.y - minY) / spanY) * 10,
  }));
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

async function saveFigureSvg(figure: HTMLElement, figureId: string): Promise<void> {
  const svg = figure.querySelector<SVGSVGElement>(".cw-plot-svg");
  if (!svg) return;
  const path = await save({
    defaultPath: `${figureId}.svg`,
    filters: [{ name: "SVG", extensions: ["svg"] }],
  });
  if (path === null) return;
  try {
    await invoke("save_svg_file_command", {
      path,
      content: serializeSvgWithResolvedTokens(svg),
    });
    dom.statusBar.textContent = `Saved SVG: ${path}`;
  } catch (error) {
    dom.statusBar.textContent = `SVG export error: ${error}`;
    console.error("SVG export error", error);
  }
}
