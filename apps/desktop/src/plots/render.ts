import {
  estimateLabelBox,
  labelLineHeight,
  labelTextLines,
  placeAxisTickLabels,
} from "./model";
import type {
  CoverageGeometry,
  DispersionGeometry,
  DispersionPlaneGeometry,
  DispersionRosterGeometry,
  LengthHistogramGeometry,
  LexicalSpectrumGeometry,
  MattrDistributionGeometry,
  PlotBarGeometry,
  PlotBox,
  PlotBracketGeometry,
  PlotLabelGeometry,
  PlotLegendItem,
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
  renderSvgLegend(svg, geometry.box, geometry.legend);
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
  renderLexicalSpectrumBands(svg, geometry);
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
  for (const divider of geometry.dividers) renderLine(svg, divider);
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

export function renderDispersionPlaneSvg(plotId: string, geometry: DispersionPlaneGeometry): SVGSVGElement {
  const svg = createBaseSvg(plotId, geometry.box);
  renderDefinitions(svg, plotId);
  renderAxes(svg, geometry.box, geometry.xScale, geometry.yScale, geometry.xTicks, geometry.yTicks, {
    x: "TOKEN FREQUENCY",
    y: "DISPERSION (DP) · 0 EVEN — 1 CONCENTRATED",
    valueAxis: "y",
  });
  renderDensityCells(svg, geometry);
  renderLine(svg, geometry.thresholdLine);
  renderQuadrantPills(svg, geometry);
  renderLabels(svg, [geometry.thresholdLabel]);
  renderPlaneWitnesses(svg, geometry);
  renderLabels(svg, geometry.labels);
  renderInteractionLayer(svg, geometry.box);
  return svg;
}

export function renderDispersionRosterSvg(plotId: string, geometry: DispersionRosterGeometry): SVGSVGElement {
  const svg = createBaseSvg(plotId, geometry.box);
  renderDefinitions(svg, plotId);
  renderAxes(svg, geometry.box, geometry.xScale, null, geometry.xTicks, [], {
    x: "DP",
    y: "",
    valueAxis: "x",
  });
  for (const row of geometry.rows) renderLine(svg, row.guide);
  renderLine(svg, geometry.thresholdLine);
  renderLabels(svg, [...geometry.rows.map((row) => row.tokenLabel), geometry.thresholdLabel]);
  renderRosterDots(svg, geometry);
  renderInteractionLayer(svg, geometry.box);
  return svg;
}

export function renderLengthHistogramSvg(plotId: string, geometry: LengthHistogramGeometry): SVGSVGElement {
  const svg = createBaseSvg(plotId, geometry.box);
  renderDefinitions(svg, plotId);
  renderAxes(svg, geometry.box, geometry.xScale, geometry.yScale, geometry.xTicks, geometry.yTicks, {
    x: "tokens per document",
    y: "documents",
    valueAxis: "y",
  });
  renderBars(svg, geometry.bars, "cw-plot-bar");
  if (geometry.medianLine) renderLine(svg, geometry.medianLine);
  renderLabels(svg, geometry.labels);
  renderInteractionLayer(svg, geometry.box);
  return svg;
}

export function renderMattrDistributionSvg(plotId: string, geometry: MattrDistributionGeometry): SVGSVGElement {
  const svg = createBaseSvg(plotId, geometry.box);
  renderDefinitions(svg, plotId);
  renderAxes(svg, geometry.box, geometry.xScale, geometry.yScale, geometry.xTicks, geometry.yTicks, {
    x: "MATTR",
    y: "documents",
    valueAxis: "y",
  });
  renderBars(svg, geometry.bars, "cw-plot-bar");
  if (geometry.medianLine) renderLine(svg, geometry.medianLine);
  renderMattrWitnessTicks(svg, geometry);
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
    for (const property of [
      "fill",
      "fill-opacity",
      "filter",
      "opacity",
      "stop-color",
      "stop-opacity",
      "stroke",
      "stroke-dasharray",
      "stroke-dashoffset",
      "stroke-linecap",
      "stroke-linejoin",
      "stroke-opacity",
      "stroke-width",
      "color",
      "font-family",
      "font-size",
      "font-weight",
    ]) {
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
  setAttrs(blur, { stdDeviation: 3.2 });
  filter.appendChild(blur);

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
  const visibleLabels = placeAxisTickLabels(ticks, axis, box);
  const visibleByTick = new Map(visibleLabels.map((label) => [label.tick, label]));
  for (const tick of ticks) {
    if (tick.major && (tick.label || tick.accessibleLabel)) renderAxisTickMark(svg, tick, axis, box);
    if (!tick.major && (scaleKind === "log" || scaleKind === "linear")) {
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
    const label = visibleByTick.get(tick);
    if (!label) continue;
    const text = createSvgElement("text");
    setAttrs(text, {
      x: label.x,
      y: label.y,
      class: "cw-plot-tick-label",
      "text-anchor": label.anchor,
    });
    text.textContent = label.text;
    svg.appendChild(text);
  }
}

function renderAxisTickMark(svg: SVGSVGElement, tick: PlotTick, axis: "x" | "y", box: PlotBox): void {
  const mark = createSvgElement("line");
  if (axis === "x") {
    setAttrs(mark, {
      x1: tick.position,
      y1: box.height - box.margin.bottom,
      x2: tick.position,
      y2: box.height - box.margin.bottom + 4,
      class: "cw-plot-axis-tick",
    });
  } else {
    setAttrs(mark, {
      x1: box.margin.left - 4,
      y1: tick.position,
      x2: box.margin.left,
      y2: tick.position,
      class: "cw-plot-axis-tick",
    });
  }
  const title = createSvgElement("title");
  title.textContent = tick.accessibleLabel ?? tick.label;
  mark.appendChild(title);
  svg.appendChild(mark);
}

export function renderSvgLegend(svg: SVGSVGElement, box: PlotBox, items: PlotLegendItem[]): void {
  if (items.length === 0) return;
  const rowHeight = 16;
  const padding = 8;
  const maxTextLength = Math.max(...items.map((item) => item.label.length));
  const width = Math.min(box.innerWidth, Math.max(116, maxTextLength * 6.6 + 34));
  const height = items.length * rowHeight + padding * 2;
  const x = box.width - box.margin.right - width;
  const y = box.margin.top + 8;
  const group = createSvgElement("g");
  group.setAttribute("class", "cw-plot-legend");
  const background = createSvgElement("rect");
  setAttrs(background, { x, y, width, height, rx: 4, class: "cw-plot-legend-bg" });
  group.appendChild(background);
  for (const [index, item] of items.entries()) {
    const rowY = y + padding + rowHeight * index + rowHeight / 2;
    const swatch = createSvgElement("line");
    setAttrs(swatch, {
      x1: x + padding,
      y1: rowY,
      x2: x + padding + 15,
      y2: rowY,
      class: `cw-plot-legend-swatch ${item.className ?? ""} ${item.dashed ? "is-dashed" : ""}`.trim(),
    });
    const text = createSvgElement("text");
    setAttrs(text, {
      x: x + padding + 21,
      y: rowY + 4,
      class: "cw-plot-legend-label",
    });
    text.textContent = item.label;
    group.append(swatch, text);
  }
  svg.appendChild(group);
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
  if (path.className.includes("cw-plot-line-glow")) {
    const halo = createSvgElement("path");
    setAttrs(halo, {
      d: path.d,
      class: `${path.className} cw-plot-line-halo`,
      filter: `url(#${plotId}-glow)`,
    });
    svg.appendChild(halo);
  }
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
      point.className ?? "",
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
      style: `--cw-plot-stagger: ${Math.min(index, 30) * 18}ms; --cw-plot-fill-opacity: ${point.fillOpacity ?? 1};`,
    });
    group.appendChild(circle);
  });
  svg.appendChild(group);
}

function renderLexicalSpectrumBands(svg: SVGSVGElement, geometry: LexicalSpectrumGeometry): void {
  const group = createSvgElement("g");
  group.setAttribute("class", "cw-plot-spectrum-bands");
  for (const band of geometry.bands) {
    const rect = createSvgElement("rect");
    setAttrs(rect, {
      x: band.x,
      y: geometry.box.margin.top,
      width: band.width,
      height: geometry.box.innerHeight,
      class: `cw-plot-spectrum-band ${band.alternate ? "is-alternate" : ""}`.trim(),
    });
    const label = createSvgElement("text");
    setAttrs(label, {
      x: band.x + (band.width / 2),
      y: geometry.box.margin.top + 14,
      class: "cw-plot-spectrum-band-label",
      "text-anchor": "middle",
    });
    label.textContent = band.label;
    group.append(rect, label);
  }
  svg.appendChild(group);
}

function renderDensityCells(svg: SVGSVGElement, geometry: DispersionPlaneGeometry): void {
  const group = createSvgElement("g");
  group.setAttribute("class", "cw-plot-density-cells");
  for (const cell of geometry.cells) {
    const rect = createSvgElement("rect");
    setAttrs(rect, {
      x: cell.x,
      y: cell.y,
      width: cell.width,
      height: cell.height,
      rx: 1.5,
      class: "cw-plot-density-cell cw-plot-hover-target",
      tabindex: "0",
      "data-plot-x": cell.x + cell.width / 2,
      "data-plot-y": cell.y + cell.height / 2,
      "data-tooltip-title": cell.tooltipTitle,
      "data-tooltip-rows": cell.tooltipRows.join("\n"),
      style: `fill: ${cell.fill}; opacity: ${cell.opacity};`,
    });
    group.appendChild(rect);
  }
  svg.appendChild(group);
}

function renderQuadrantPills(svg: SVGSVGElement, geometry: DispersionPlaneGeometry): void {
  const group = createSvgElement("g");
  group.setAttribute("class", "cw-plot-quadrant-pills");
  for (const pill of geometry.quadrantPills) {
    const rect = createSvgElement("rect");
    setAttrs(rect, {
      x: pill.box.x,
      y: pill.box.y,
      width: pill.box.width,
      height: pill.box.height,
      rx: 10,
      class: "cw-plot-quadrant-pill-bg",
    });
    group.appendChild(rect);
  }
  svg.appendChild(group);
  renderLabels(svg, geometry.quadrantPills.map((pill) => pill.label));
}

function renderPlaneWitnesses(svg: SVGSVGElement, geometry: DispersionPlaneGeometry): void {
  const group = createSvgElement("g");
  group.setAttribute("class", "cw-plot-plane-witnesses");
  geometry.witnesses.forEach((witness, index) => {
    const circle = createSvgElement("circle");
    setAttrs(circle, {
      cx: witness.x,
      cy: witness.y,
      r: 5,
      class: "cw-plot-plane-witness cw-plot-hover-target",
      tabindex: "0",
      "data-plot-x": witness.x,
      "data-plot-y": witness.y,
      "data-tooltip-title": witness.tooltipTitle ?? "",
      "data-tooltip-rows": (witness.tooltipRows ?? []).join("\n"),
      style: `--cw-plot-stagger: ${Math.min(index, 30) * 18}ms;`,
    });
    group.appendChild(circle);
  });
  svg.appendChild(group);
}

function renderRosterDots(svg: SVGSVGElement, geometry: DispersionRosterGeometry): void {
  const group = createSvgElement("g");
  group.setAttribute("class", "cw-plot-roster-dots");
  geometry.rows.forEach((row, index) => {
    const point = row.point;
    const circle = createSvgElement("circle");
    setAttrs(circle, {
      cx: point.x,
      cy: point.y,
      r: 4.5,
      class: [
        "cw-plot-roster-dot",
        row.aboveThreshold ? "cw-plot-ramp-5" : "",
        "cw-plot-hover-target",
      ].filter(Boolean).join(" "),
      tabindex: "0",
      "data-plot-x": point.x,
      "data-plot-y": point.y,
      "data-tooltip-title": point.tooltipTitle ?? "",
      "data-tooltip-rows": (point.tooltipRows ?? []).join("\n"),
      style: `--cw-plot-stagger: ${Math.min(index, 30) * 12}ms;`,
    });
    group.appendChild(circle);
  });
  svg.appendChild(group);
}

function renderBars(svg: SVGSVGElement, bars: Array<PlotBarGeometry>, baseClass: string): void {
  const group = createSvgElement("g");
  group.setAttribute("class", "cw-plot-bars");
  bars.forEach((bar, index) => {
    const rect = createSvgElement("rect");
    setAttrs(rect, {
      x: bar.x,
      y: bar.y,
      width: bar.width,
      height: bar.height,
      rx: 2,
      class: `${baseClass} cw-plot-hover-target`,
      tabindex: "0",
      "data-plot-x": bar.x + bar.width / 2,
      "data-plot-y": bar.y,
      "data-tooltip-title": bar.tooltipTitle ?? "",
      "data-tooltip-rows": (bar.tooltipRows ?? []).join("\n"),
      style: `--cw-plot-stagger: ${Math.min(index, 30) * 14}ms;`,
    });
    group.appendChild(rect);
  });
  svg.appendChild(group);
}

function renderMattrWitnessTicks(svg: SVGSVGElement, geometry: MattrDistributionGeometry): void {
  const group = createSvgElement("g");
  group.setAttribute("class", "cw-plot-mattr-witnesses");
  geometry.witnessTicks.forEach((witness) => {
    const line = createSvgElement("line");
    setAttrs(line, {
      x1: witness.x,
      y1: witness.y1,
      x2: witness.x,
      y2: witness.y2,
      class: "cw-plot-mattr-witness-tick cw-plot-hover-target",
      tabindex: "0",
      "data-plot-x": witness.x,
      "data-plot-y": witness.y2,
      "data-tooltip-title": witness.tooltipTitle,
      "data-tooltip-rows": witness.tooltipRows.join("\n"),
    });
    group.appendChild(line);
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
    const lines = labelTextLines(label);
    if (lines.length === 1) {
      text.textContent = lines[0] ?? "";
    } else {
      lines.forEach((line, index) => {
        const tspan = createSvgElement("tspan");
        setAttrs(tspan, {
          x: label.x,
          y: label.y + index * labelLineHeight(label),
        });
        tspan.textContent = line;
        text.appendChild(tspan);
      });
    }
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
  const labelBox = estimateLabelBox(label);
  const targetX = Math.max(labelBox.x, Math.min(last.x, labelBox.x + labelBox.width));
  renderLine(svg, {
    x1: last.x + 4,
    y1: last.y + 4,
    x2: targetX,
    y2: labelBox.y - 4,
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
