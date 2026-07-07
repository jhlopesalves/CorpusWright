import {
  attachPlotHover,
  serializeSvgWithResolvedTokens,
} from "./render";

const FIGURE_COLLAPSE_STORAGE_KEY = "corpuswright.overview.figureCollapse";
const SVG_NS = "http://www.w3.org/2000/svg";

export interface OverviewFigureOptions {
  id: string;
  title: string;
  subtitle: string;
  captionStats: string;
  captionReading: string;
  controls?: HTMLElement;
  renderSvg: () => SVGSVGElement;
}

export interface SegmentedControlOption<TValue extends string> {
  value: TValue;
  label: string;
}

export function createOverviewFigure(options: OverviewFigureOptions): HTMLElement {
  const figure = document.createElement("section");
  figure.className = "cw-overview-figure";
  figure.dataset.figureId = options.id;
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
  exportButton.className = "secondary-btn cw-overview-figure-icon-btn cw-overview-figure-export";
  exportButton.title = "Export SVG";
  exportButton.setAttribute("aria-label", `Export ${options.title} as SVG`);
  exportButton.textContent = "SVG";

  const collapseButton = document.createElement("button");
  collapseButton.type = "button";
  collapseButton.className = "secondary-btn cw-overview-figure-toggle cw-overview-figure-collapse";
  collapseButton.title = "Collapse figure";
  collapseButton.setAttribute("aria-label", `Collapse ${options.title}`);

  const collapseLabel = document.createElement("span");
  collapseLabel.className = "cw-overview-figure-toggle-label";
  collapseLabel.textContent = "Hide";

  const chevron = document.createElementNS(SVG_NS, "svg");
  chevron.classList.add("cw-overview-figure-chevron");
  chevron.setAttribute("aria-hidden", "true");
  chevron.setAttribute("viewBox", "0 0 12 8");
  chevron.setAttribute("width", "12");
  chevron.setAttribute("height", "8");
  const chevronPath = document.createElementNS(SVG_NS, "path");
  chevronPath.setAttribute("d", "M1 1.5 6 6.5 11 1.5");
  chevronPath.setAttribute("fill", "none");
  chevronPath.setAttribute("stroke", "currentColor");
  chevronPath.setAttribute("stroke-width", "1.8");
  chevronPath.setAttribute("stroke-linecap", "round");
  chevronPath.setAttribute("stroke-linejoin", "round");
  chevron.appendChild(chevronPath);
  collapseButton.append(collapseLabel, chevron);
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
  const reading = document.createElement("p");
  reading.className = "cw-overview-figure-caption-reading";
  reading.textContent = options.captionReading;
  caption.append(stats, reading);

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

  exportButton.addEventListener("click", () => {
    const svg = figure.querySelector<SVGSVGElement>(".cw-plot-svg");
    if (!svg) return;
    downloadSvg(`${options.id}.svg`, serializeSvgWithResolvedTokens(svg));
  });

  figure.append(header, body, caption);
  return figure;
}

export function replaceFigureSvg(figure: HTMLElement, svg: SVGSVGElement): void {
  const mount = figure.querySelector<HTMLElement>("[data-figure-svg-mount='true']");
  if (!mount) return;
  replaceFigureSvgElement(mount, svg);
  const sparkline = figure.querySelector<HTMLElement>("[data-figure-sparkline='true']");
  if (sparkline) renderSparklineGlyph(sparkline, svg);
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
  button.title = collapsed ? "Show plot" : "Hide plot";
  button.setAttribute("aria-label", collapsed ? `Show plot for ${title}` : `Hide plot for ${title}`);
  label.textContent = collapsed ? "Show plot" : "Hide";
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

function downloadSvg(filename: string, contents: string): void {
  const blob = new Blob([contents], { type: "image/svg+xml;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.style.display = "none";
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}
