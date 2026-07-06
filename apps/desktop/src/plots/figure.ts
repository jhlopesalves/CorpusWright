import {
  attachPlotHover,
  serializeSvgWithResolvedTokens,
} from "./render";

const FIGURE_COLLAPSE_STORAGE_KEY = "corpuswright.overview.figureCollapse";

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

  const header = document.createElement("div");
  header.className = "cw-overview-figure-header";

  const titleBlock = document.createElement("div");
  titleBlock.className = "cw-overview-figure-title-block";
  const title = document.createElement("h4");
  title.textContent = options.title;
  const subtitle = document.createElement("p");
  subtitle.textContent = options.subtitle;
  titleBlock.append(title, subtitle);

  const actions = document.createElement("div");
  actions.className = "cw-overview-figure-actions";
  if (options.controls) {
    actions.appendChild(options.controls);
  }

  const badgeSlot = document.createElement("span");
  badgeSlot.className = "cw-overview-figure-badge-slot";
  badgeSlot.setAttribute("aria-hidden", "true");

  const exportButton = document.createElement("button");
  exportButton.type = "button";
  exportButton.className = "secondary-btn cw-overview-figure-icon-btn";
  exportButton.title = "Export SVG";
  exportButton.setAttribute("aria-label", `Export ${options.title} as SVG`);
  exportButton.textContent = "SVG";

  const collapseButton = document.createElement("button");
  collapseButton.type = "button";
  collapseButton.className = "secondary-btn cw-overview-figure-icon-btn cw-overview-figure-collapse";
  collapseButton.title = "Collapse figure";
  collapseButton.setAttribute("aria-label", `Collapse ${options.title}`);

  const chevron = document.createElement("span");
  chevron.className = "cw-overview-figure-chevron";
  chevron.setAttribute("aria-hidden", "true");
  chevron.textContent = "v";
  collapseButton.appendChild(chevron);
  actions.append(badgeSlot, exportButton, collapseButton);
  header.append(titleBlock, actions);

  const body = document.createElement("div");
  body.className = "cw-overview-figure-body";
  body.dataset.figureSvgMount = "true";
  replaceFigureSvgElement(body, options.renderSvg());

  const caption = document.createElement("figcaption");
  caption.className = "cw-overview-figure-caption";
  const stats = document.createElement("p");
  stats.className = "cw-overview-figure-caption-stats";
  stats.textContent = options.captionStats;
  const reading = document.createElement("p");
  reading.className = "cw-overview-figure-caption-reading";
  reading.textContent = options.captionReading;
  caption.append(stats, reading);

  const collapsed = readCollapsedFigures().has(options.id);
  setFigureCollapsed(figure, body, caption, collapseButton, collapsed);

  collapseButton.addEventListener("click", () => {
    const next = !figure.classList.contains("is-collapsed");
    const collapsedFigures = readCollapsedFigures();
    if (next) collapsedFigures.add(options.id);
    else collapsedFigures.delete(options.id);
    writeCollapsedFigures(collapsedFigures);
    setFigureCollapsed(figure, body, caption, collapseButton, next);
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
  body: HTMLElement,
  caption: HTMLElement,
  button: HTMLButtonElement,
  collapsed: boolean,
): void {
  figure.classList.toggle("is-collapsed", collapsed);
  body.hidden = collapsed;
  caption.hidden = collapsed;
  button.setAttribute("aria-expanded", String(!collapsed));
  button.title = collapsed ? "Expand figure" : "Collapse figure";
  button.setAttribute("aria-label", collapsed ? "Expand figure" : "Collapse figure");
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
