import type {
  CoverageGeometry,
  DispersionGeometry,
  LexicalSpectrumGeometry,
  PlotBox,
  PlotBracketGeometry,
  PlotLabelGeometry,
  PlotLineGeometry,
  PlotPathGeometry,
  PlotPointGeometry,
  PlotScale,
  PlotTick,
  RankFrequencyGeometry,
  VocabularyGrowthGeometry,
} from "./model";

const SVG_NS = "http://www.w3.org/2000/svg";

export function renderRankFrequencySvg(plotId: string, geometry: RankFrequencyGeometry, yAxisLabel: string): SVGSVGElement {
  const svg = createBaseSvg(plotId, geometry.box);
  renderDefinitions(svg, plotId);
  renderAxes(svg, geometry.box, geometry.xScale, geometry.yScale, geometry.xTicks, geometry.yTicks, {
    x: "rank",
    y: yAxisLabel,
    valueAxis: "y",
  });
  if (geometry.fitPath) renderPath(svg, geometry.fitPath, plotId);
  renderPoints(svg, geometry.points, "cw-plot-point");
  renderLabels(svg, geometry.labels);
  if (geometry.bracket) renderBracket(svg, geometry.bracket);
  renderInteractionLayer(svg, geometry.box);
  return svg;
}

export function renderVocabularyGrowthSvg(plotId: string, geometry: VocabularyGrowthGeometry): SVGSVGElement {
  const svg = createBaseSvg(plotId, geometry.box);
  renderDefinitions(svg, plotId);
  renderAxes(svg, geometry.box, geometry.xScale, geometry.yScale, geometry.xTicks, geometry.yTicks, {
    x: "tokens",
    y: "types",
    valueAxis: "y",
  });
  if (geometry.areaPath) renderPath(svg, geometry.areaPath, plotId);
  if (geometry.linePath) renderPath(svg, geometry.linePath, plotId);
  renderPoints(svg, geometry.points, "cw-plot-point is-panel-stroked");
  renderLabels(svg, geometry.labels);
  renderEndpointLeader(svg, geometry.points, geometry.labels[0]);
  renderInteractionLayer(svg, geometry.box);
  return svg;
}

export function renderLexicalSpectrumSvg(plotId: string, geometry: LexicalSpectrumGeometry): SVGSVGElement {
  const svg = createBaseSvg(plotId, geometry.box);
  renderDefinitions(svg, plotId);
  renderAxes(svg, geometry.box, geometry.xScale, geometry.yScale, geometry.xTicks, geometry.yTicks, {
    x: "frequency",
    y: "types",
    valueAxis: "y",
  });
  renderPoints(svg, geometry.points, "cw-plot-point");
  renderLabels(svg, geometry.labels);
  renderInteractionLayer(svg, geometry.box);
  return svg;
}

export function renderCoverageSvg(plotId: string, geometry: CoverageGeometry): SVGSVGElement {
  const svg = createBaseSvg(plotId, geometry.box);
  renderDefinitions(svg, plotId);
  renderAxes(svg, geometry.box, geometry.xScale, geometry.yScale, geometry.xTicks, geometry.yTicks, {
    x: "types by rank",
    y: "token coverage",
    valueAxis: "y",
  });
  if (geometry.areaPath) renderPath(svg, geometry.areaPath, plotId);
  if (geometry.linePath) renderPath(svg, geometry.linePath, plotId);
  renderPoints(svg, geometry.milestones, "cw-plot-point is-panel-stroked");
  if (geometry.halfCrossing) {
    renderLine(svg, {
      x1: geometry.box.margin.left,
      y1: geometry.halfCrossing.y,
      x2: geometry.halfCrossing.x,
      y2: geometry.halfCrossing.y,
      className: "cw-plot-reference-line",
    });
    renderLine(svg, {
      x1: geometry.halfCrossing.x,
      y1: geometry.halfCrossing.y,
      x2: geometry.halfCrossing.x,
      y2: geometry.box.height - geometry.box.margin.bottom,
      className: "cw-plot-reference-line",
    });
  }
  renderLabels(svg, geometry.labels);
  renderInteractionLayer(svg, geometry.box);
  return svg;
}

export function renderDispersionSvg(plotId: string, geometry: DispersionGeometry): SVGSVGElement {
  const svg = createBaseSvg(plotId, geometry.box);
  renderDefinitions(svg, plotId);
  renderAxes(svg, geometry.box, geometry.xScale, null, geometry.xTicks, [], {
    x: "types",
    y: "",
    valueAxis: "x",
  });
  for (const stem of geometry.stems) renderLine(svg, stem);
  renderPoints(svg, geometry.points, "cw-plot-point is-panel-stroked is-lollipop");
  for (const point of geometry.points.filter((candidate) => candidate.datum.highlighted)) {
    const halo = createSvgElement("circle");
    setAttrs(halo, {
      cx: point.x,
      cy: point.y,
      r: 9,
      class: `cw-plot-halo cw-plot-ramp-${point.rampIndex}`,
    });
    svg.appendChild(halo);
  }
  renderLabels(svg, geometry.labels);
  renderInteractionLayer(svg, geometry.box);
  return svg;
}

export function attachPlotHover(svg: SVGSVGElement): void {
  const crosshairX = svg.querySelector<SVGLineElement>(".cw-plot-crosshair-x");
  const crosshairY = svg.querySelector<SVGLineElement>(".cw-plot-crosshair-y");
  const tooltip = svg.querySelector<SVGGElement>(".cw-plot-tooltip");
  const tooltipText = svg.querySelector<SVGTextElement>(".cw-plot-tooltip-text");
  const tooltipBox = svg.querySelector<SVGRectElement>(".cw-plot-tooltip-box");
  if (!crosshairX || !crosshairY || !tooltip || !tooltipText || !tooltipBox) return;

  const hide = () => {
    svg.classList.remove("is-hovering");
    tooltip.setAttribute("aria-hidden", "true");
  };
  svg.addEventListener("pointerleave", hide);
  svg.addEventListener("focusout", (event) => {
    if (!svg.contains(event.relatedTarget as Node | null)) hide();
  });
  svg.addEventListener("pointerover", (event) => updateHover(svg, event.target, crosshairX, crosshairY, tooltip, tooltipText, tooltipBox));
  svg.addEventListener("focusin", (event) => updateHover(svg, event.target, crosshairX, crosshairY, tooltip, tooltipText, tooltipBox));
}

export function serializeSvgWithResolvedTokens(svg: SVGSVGElement): string {
  const clone = svg.cloneNode(true) as SVGSVGElement;
  const originalElements = [svg, ...Array.from(svg.querySelectorAll<SVGElement>("*"))];
  const cloneElements = [clone, ...Array.from(clone.querySelectorAll<SVGElement>("*"))];
  for (let index = 0; index < originalElements.length; index += 1) {
    const original = originalElements[index]!;
    const cloned = cloneElements[index]!;
    const style = window.getComputedStyle(original);
    for (const property of ["fill", "stroke", "color", "font-family", "font-size", "font-weight", "opacity", "stroke-width"]) {
      const value = style.getPropertyValue(property);
      if (value) cloned.style.setProperty(property, value);
    }
  }
  clone.setAttribute("xmlns", SVG_NS);
  return new XMLSerializer().serializeToString(clone);
}

function createBaseSvg(plotId: string, box: PlotBox): SVGSVGElement {
  const svg = createSvgElement("svg");
  setAttrs(svg, {
    class: "cw-plot-svg",
    role: "img",
    viewBox: `0 0 ${box.width} ${box.height}`,
    "data-plot-id": plotId,
  });
  return svg;
}

function renderDefinitions(svg: SVGSVGElement, plotId: string): void {
  const defs = createSvgElement("defs");

  const filter = createSvgElement("filter");
  setAttrs(filter, {
    id: `${plotId}-glow`,
    x: "-12%",
    y: "-20%",
    width: "124%",
    height: "140%",
  });
  const blur = createSvgElement("feGaussianBlur");
  setAttrs(blur, { stdDeviation: 3, result: "blur" });
  const merge = createSvgElement("feMerge");
  merge.appendChild(createSvgElement("feMergeNode"));
  const crisp = createSvgElement("feMergeNode");
  crisp.setAttribute("in", "SourceGraphic");
  merge.appendChild(crisp);
  filter.append(blur, merge);

  const gradient = createSvgElement("linearGradient");
  setAttrs(gradient, {
    id: `${plotId}-area`,
    x1: "0",
    y1: "0",
    x2: "0",
    y2: "1",
  });
  for (const [offset, opacity] of [["0%", "0.30"], ["68%", "0.06"], ["100%", "0"]] as const) {
    const stop = createSvgElement("stop");
    setAttrs(stop, {
      offset,
      class: "cw-plot-area-stop",
      "stop-opacity": opacity,
    });
    gradient.appendChild(stop);
  }

  defs.append(filter, gradient);
  svg.appendChild(defs);
}

function renderAxes(
  svg: SVGSVGElement,
  box: PlotBox,
  xScale: PlotScale,
  yScale: PlotScale | null,
  xTicks: PlotTick[],
  yTicks: PlotTick[],
  labels: { x: string; y: string; valueAxis: "x" | "y" },
): void {
  const grid = createSvgElement("g");
  grid.setAttribute("class", "cw-plot-grid");
  for (const tick of yTicks.filter((candidate) => candidate.major)) {
    const line = createSvgElement("line");
    setAttrs(line, {
      x1: box.margin.left,
      y1: tick.position,
      x2: box.width - box.margin.right,
      y2: tick.position,
      class: labels.valueAxis === "y" ? "cw-plot-grid-value" : "cw-plot-grid-context",
    });
    grid.appendChild(line);
  }
  for (const tick of xTicks.filter((candidate) => candidate.major)) {
    const line = createSvgElement("line");
    setAttrs(line, {
      x1: tick.position,
      y1: box.margin.top,
      x2: tick.position,
      y2: box.height - box.margin.bottom,
      class: labels.valueAxis === "x" ? "cw-plot-grid-value" : "cw-plot-grid-context",
    });
    grid.appendChild(line);
  }
  svg.appendChild(grid);

  renderLine(svg, {
    x1: box.margin.left,
    y1: box.height - box.margin.bottom,
    x2: box.width - box.margin.right,
    y2: box.height - box.margin.bottom,
    className: "cw-plot-axis-baseline",
  });
  renderLine(svg, {
    x1: box.margin.left,
    y1: box.margin.top,
    x2: box.margin.left,
    y2: box.height - box.margin.bottom,
    className: "cw-plot-axis-baseline",
  });

  renderTickLabels(svg, xTicks, "x", box, xScale.kind);
  if (yScale) renderTickLabels(svg, yTicks, "y", box, yScale.kind);
  renderAxisTitles(svg, box, labels.x, labels.y);
}

function renderTickLabels(
  svg: SVGSVGElement,
  ticks: PlotTick[],
  axis: "x" | "y",
  box: PlotBox,
  scaleKind: "linear" | "log",
): void {
  for (const tick of ticks) {
    if (!tick.major && scaleKind === "log") {
      const mark = createSvgElement("line");
      if (axis === "x") {
        setAttrs(mark, {
          x1: tick.position,
          y1: box.height - box.margin.bottom,
          x2: tick.position,
          y2: box.height - box.margin.bottom + 4,
          class: "cw-plot-minor-tick",
        });
      } else {
        setAttrs(mark, {
          x1: box.margin.left - 4,
          y1: tick.position,
          x2: box.margin.left,
          y2: tick.position,
          class: "cw-plot-minor-tick",
        });
      }
      svg.appendChild(mark);
      continue;
    }
    if (!tick.label) continue;
    const text = createSvgElement("text");
    if (axis === "x") {
      setAttrs(text, {
        x: tick.position,
        y: box.height - box.margin.bottom + 19,
        class: "cw-plot-tick-label",
        "text-anchor": "middle",
      });
    } else {
      setAttrs(text, {
        x: box.margin.left - 10,
        y: tick.position + 4,
        class: "cw-plot-tick-label",
        "text-anchor": "end",
      });
    }
    text.textContent = tick.label;
    svg.appendChild(text);
  }
}

function renderAxisTitles(svg: SVGSVGElement, box: PlotBox, xLabel: string, yLabel: string): void {
  const xTitle = createSvgElement("text");
  setAttrs(xTitle, {
    x: box.margin.left + box.innerWidth / 2,
    y: box.height - 13,
    class: "cw-plot-axis-title",
    "text-anchor": "middle",
  });
  xTitle.textContent = xLabel;
  svg.appendChild(xTitle);

  if (!yLabel) return;
  const yTitle = createSvgElement("text");
  setAttrs(yTitle, {
    x: 16,
    y: box.margin.top + box.innerHeight / 2,
    class: "cw-plot-axis-title",
    transform: `rotate(-90 16 ${box.margin.top + box.innerHeight / 2})`,
    "text-anchor": "middle",
  });
  yTitle.textContent = yLabel;
  svg.appendChild(yTitle);
}

function renderPath(svg: SVGSVGElement, path: PlotPathGeometry, plotId: string): void {
  const element = createSvgElement("path");
  const className = path.className.includes("cw-plot-area") ? path.className : `${path.className} cw-plot-animated-path`;
  setAttrs(element, {
    d: path.d,
    class: className,
    pathLength: "1",
  });
  if (path.className.includes("cw-plot-area")) {
    element.setAttribute("fill", `url(#${plotId}-area)`);
  }
  if (path.className.includes("cw-plot-line-glow")) {
    element.setAttribute("filter", `url(#${plotId}-glow)`);
  }
  svg.appendChild(element);
}

function renderPoints(svg: SVGSVGElement, points: Array<PlotPointGeometry>, baseClass: string): void {
  const group = createSvgElement("g");
  group.setAttribute("class", "cw-plot-points");
  points.forEach((point, index) => {
    const circle = createSvgElement("circle");
    const className = [
      baseClass,
      `cw-plot-ramp-${point.rampIndex}`,
      point.excluded ? "is-excluded" : "",
      "cw-plot-hover-target",
    ].filter(Boolean).join(" ");
    setAttrs(circle, {
      cx: point.x,
      cy: point.y,
      r: baseClass.includes("is-lollipop") ? 5 : 4.2,
      class: className,
      tabindex: "0",
      "data-plot-x": point.x,
      "data-plot-y": point.y,
      "data-tooltip-title": point.tooltipTitle ?? "",
      "data-tooltip-rows": (point.tooltipRows ?? []).join("\n"),
      style: `--cw-plot-stagger: ${Math.min(index, 30) * 18}ms;`,
    });
    group.appendChild(circle);
  });
  svg.appendChild(group);
}

function renderLabels(svg: SVGSVGElement, labels: PlotLabelGeometry[]): void {
  for (const label of labels) {
    const text = createSvgElement("text");
    setAttrs(text, {
      x: label.x,
      y: label.y,
      class: `cw-plot-label ${label.className ?? ""}`.trim(),
      "text-anchor": label.anchor,
    });
    text.textContent = label.text;
    svg.appendChild(text);
  }
}

function renderBracket(svg: SVGSVGElement, bracket: PlotBracketGeometry): void {
  const group = createSvgElement("g");
  group.setAttribute("class", "cw-plot-bracket");
  for (const line of [
    { x1: bracket.x1, y1: bracket.y, x2: bracket.x2, y2: bracket.y },
    { x1: bracket.x1, y1: bracket.y - 5, x2: bracket.x1, y2: bracket.y + 5 },
    { x1: bracket.x2, y1: bracket.y - 5, x2: bracket.x2, y2: bracket.y + 5 },
  ]) {
    const element = createSvgElement("line");
    setAttrs(element, line);
    group.appendChild(element);
  }
  const label = createSvgElement("text");
  setAttrs(label, {
    x: bracket.x2,
    y: bracket.y + 16,
    "text-anchor": "end",
  });
  label.textContent = bracket.label;
  group.appendChild(label);
  svg.appendChild(group);
}

function renderEndpointLeader(
  svg: SVGSVGElement,
  points: Array<PlotPointGeometry>,
  label: PlotLabelGeometry | undefined,
): void {
  const last = points[points.length - 1];
  if (!last || !label) return;
  renderLine(svg, {
    x1: last.x + 4,
    y1: last.y + 4,
    x2: label.x - 5,
    y2: label.y - 5,
    className: "cw-plot-leader",
  });
}

function renderInteractionLayer(svg: SVGSVGElement, box: PlotBox): void {
  const group = createSvgElement("g");
  group.setAttribute("class", "cw-plot-interaction-layer");
  group.setAttribute("aria-hidden", "true");
  const vertical = createSvgElement("line");
  setAttrs(vertical, {
    class: "cw-plot-crosshair-x",
    y1: box.margin.top,
    y2: box.height - box.margin.bottom,
  });
  const horizontal = createSvgElement("line");
  setAttrs(horizontal, {
    class: "cw-plot-crosshair-y",
    x1: box.margin.left,
    x2: box.width - box.margin.right,
  });
  const tooltip = createSvgElement("g");
  tooltip.setAttribute("class", "cw-plot-tooltip");
  tooltip.setAttribute("aria-hidden", "true");
  const tooltipBox = createSvgElement("rect");
  tooltipBox.setAttribute("class", "cw-plot-tooltip-box");
  const tooltipText = createSvgElement("text");
  tooltipText.setAttribute("class", "cw-plot-tooltip-text");
  tooltip.append(tooltipBox, tooltipText);
  group.append(vertical, horizontal, tooltip);
  svg.appendChild(group);
}

function renderLine(svg: SVGSVGElement, line: PlotLineGeometry): void {
  const element = createSvgElement("line");
  setAttrs(element, {
    x1: line.x1,
    y1: line.y1,
    x2: line.x2,
    y2: line.y2,
    class: line.className,
  });
  svg.appendChild(element);
}

function updateHover(
  svg: SVGSVGElement,
  target: EventTarget | null,
  crosshairX: SVGLineElement,
  crosshairY: SVGLineElement,
  tooltip: SVGGElement,
  tooltipText: SVGTextElement,
  tooltipBox: SVGRectElement,
): void {
  const marker = target instanceof Element ? target.closest<SVGElement>(".cw-plot-hover-target") : null;
  if (!marker || !svg.contains(marker)) return;
  const x = Number(marker.dataset.plotX);
  const y = Number(marker.dataset.plotY);
  if (!Number.isFinite(x) || !Number.isFinite(y)) return;
  svg.classList.add("is-hovering");
  crosshairX.setAttribute("x1", String(x));
  crosshairX.setAttribute("x2", String(x));
  crosshairY.setAttribute("y1", String(y));
  crosshairY.setAttribute("y2", String(y));
  tooltipText.replaceChildren();
  const rows = [marker.dataset.tooltipTitle ?? "", ...(marker.dataset.tooltipRows ?? "").split("\n")]
    .map((row) => row.trim())
    .filter(Boolean);
  rows.forEach((row, index) => {
    const tspan = createSvgElement("tspan");
    setAttrs(tspan, {
      x: 8,
      y: 16 + index * 15,
      class: index === 0 ? "cw-plot-tooltip-title" : "cw-plot-tooltip-row",
    });
    tspan.textContent = row;
    tooltipText.appendChild(tspan);
  });
  const width = Math.max(92, ...rows.map((row) => row.length * 7.2)) + 16;
  const height = rows.length * 15 + 10;
  setAttrs(tooltipBox, { x: 0, y: 0, width, height, rx: 6 });
  const plotWidth = Number(svg.viewBox.baseVal.width) || 720;
  const plotHeight = Number(svg.viewBox.baseVal.height) || 360;
  const tooltipX = x + width + 18 > plotWidth ? x - width - 12 : x + 12;
  const tooltipY = y + height + 18 > plotHeight ? y - height - 12 : y + 12;
  tooltip.setAttribute("transform", `translate(${tooltipX} ${tooltipY})`);
  tooltip.setAttribute("aria-hidden", "false");
}

function createSvgElement<K extends keyof SVGElementTagNameMap>(tag: K): SVGElementTagNameMap[K] {
  return document.createElementNS(SVG_NS, tag);
}

function setAttrs(element: Element, attrs: Record<string, string | number>): void {
  for (const [key, value] of Object.entries(attrs)) {
    element.setAttribute(key, String(value));
  }
}
