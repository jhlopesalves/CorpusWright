export interface MeasuredLayoutBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface AnnotationCandidate {
  dx: number;
  dy: number;
  displacementPenalty: number;
}

const SVG_NS = "http://www.w3.org/2000/svg";
const BOX_GAP = 3;

export function applyMountedFigureLayout(svg: SVGSVGElement): void {
  if (svg.dataset.measuredLayout === "true" || !svg.isConnected) return;
  svg.dataset.measuredLayout = "true";
  const original = svg.viewBox.baseVal;
  const extent = { right: original.x + original.width, bottom: original.y + original.height };
  layoutCompositionStripLabels(svg, extent);
  placeOccupancyAwareLegends(svg, extent);
  resolveMountedAnnotationCollisions(svg, extent);
  if (extent.right > original.x + original.width || extent.bottom > original.y + original.height) {
    svg.setAttribute("viewBox", `${original.x} ${original.y} ${Math.ceil(extent.right - original.x + 8)} ${Math.ceil(extent.bottom - original.y + 8)}`);
  }
  svg.dataset.layoutContentBottom = String(Math.ceil(extent.bottom));
}

export function annotationCandidates(step = 16): AnnotationCandidate[] {
  return [
    { dx: 0, dy: 0, displacementPenalty: 0 },
    { dx: step, dy: -step, displacementPenalty: 1 },
    { dx: -step, dy: -step, displacementPenalty: 1 },
    { dx: step, dy: step, displacementPenalty: 1 },
    { dx: -step, dy: step, displacementPenalty: 1 },
    { dx: step * 2, dy: 0, displacementPenalty: 2 },
    { dx: -step * 2, dy: 0, displacementPenalty: 2 },
    { dx: 0, dy: step * 2, displacementPenalty: 2 },
    { dx: 0, dy: -step * 2, displacementPenalty: 2 },
  ];
}

export function scoreLayoutCandidate(
  box: MeasuredLayoutBox,
  obstacles: MeasuredLayoutBox[],
  bounds: MeasuredLayoutBox,
  displacementPenalty = 0,
): number {
  let score = displacementPenalty;
  if (box.x < bounds.x || box.y < bounds.y || box.x + box.width > bounds.x + bounds.width || box.y + box.height > bounds.y + bounds.height) {
    score += 1_000_000;
  }
  for (const obstacle of obstacles) {
    if (!layoutBoxesOverlap(box, obstacle, BOX_GAP)) continue;
    const overlapWidth = Math.min(box.x + box.width, obstacle.x + obstacle.width) - Math.max(box.x, obstacle.x);
    const overlapHeight = Math.min(box.y + box.height, obstacle.y + obstacle.height) - Math.max(box.y, obstacle.y);
    score += 10_000 + Math.max(0, overlapWidth) * Math.max(0, overlapHeight);
  }
  return score;
}

export function layoutBoxesOverlap(left: MeasuredLayoutBox, right: MeasuredLayoutBox, gap = 0): boolean {
  return left.x < right.x + right.width + gap
    && left.x + left.width + gap > right.x
    && left.y < right.y + right.height + gap
    && left.y + left.height + gap > right.y;
}

function layoutCompositionStripLabels(
  svg: SVGSVGElement,
  extent: { right: number; bottom: number },
): void {
  const entries = Array.from(svg.querySelectorAll<SVGGElement>(".cw-plot-composition-entry"));
  if (entries.length === 0) return;
  const external: Array<{ entry: SVGGElement; mode: "medium" | "narrow" | "zero" }> = [];
  for (const entry of entries) {
    const x = Number(entry.dataset.segmentX);
    const y = Number(entry.dataset.segmentY);
    const width = Number(entry.dataset.segmentWidth);
    const height = Number(entry.dataset.segmentHeight);
    const category = entry.querySelector<SVGTextElement>(".is-category");
    const detail = entry.querySelector<SVGTextElement>(".is-detail");
    if (!category || !detail) continue;
    const categoryWidth = measuredTextWidth(category);
    const detailWidth = measuredTextWidth(detail);
    const centre = x + width / 2;
    entry.classList.remove("is-wide", "is-medium", "is-narrow", "is-zero");
    if (width <= 0) {
      entry.classList.add("is-zero");
      external.push({ entry, mode: "zero" });
      continue;
    }
    if (Math.max(categoryWidth, detailWidth) + 12 <= width) {
      entry.classList.add("is-wide");
      setTextPosition(category, centre, y + Math.max(9, height / 2 - 1), "middle");
      setTextPosition(detail, centre, y + Math.min(height - 2, height / 2 + 9), "middle");
      continue;
    }
    if (categoryWidth + 10 <= width && width >= 48) {
      entry.classList.add("is-medium");
      setTextPosition(category, centre, y + height / 2 + 4, "middle");
      external.push({ entry, mode: "medium" });
      continue;
    }
    entry.classList.add("is-narrow");
    external.push({ entry, mode: "narrow" });
  }
  if (external.length === 0) return;
  const first = entries[0]!;
  const stripX = Number(first.dataset.segmentX);
  const stripY = Number(first.dataset.segmentY);
  const stripHeight = Number(first.dataset.segmentHeight);
  const last = entries[entries.length - 1]!;
  const stripRight = Number(last.dataset.segmentX) + Number(last.dataset.segmentWidth);
  const stripWidth = stripRight - stripX;
  const columns = stripWidth >= 480 && external.length > 2 ? 2 : 1;
  const columnWidth = stripWidth / columns;
  const startY = stripY + stripHeight + 16;
  external.forEach(({ entry, mode }, index) => {
    const row = Math.floor(index / columns);
    const column = index % columns;
    const x = stripX + column * columnWidth;
    const y = startY + row * 16;
    const marker = entry.querySelector<SVGCircleElement>(".cw-plot-composition-entry-marker");
    const category = entry.querySelector<SVGTextElement>(".is-category")!;
    const detail = entry.querySelector<SVGTextElement>(".is-detail")!;
    if (marker) setCirclePosition(marker, x + 4, y - 3);
    if (mode === "medium") {
      setTextPosition(detail, x + 12, y, "start");
    } else {
      category.textContent = `${category.textContent} — ${detail.textContent}`;
      setTextPosition(category, x + 12, y, "start");
    }
    extent.bottom = Math.max(extent.bottom, y + 6);
  });
}

function placeOccupancyAwareLegends(
  svg: SVGSVGElement,
  extent: { right: number; bottom: number },
): void {
  const legends = Array.from(svg.querySelectorAll<SVGGElement>(
    ".cw-plot-legend, .cw-plot-density-legend, .cw-plot-witness-legend",
  ));
  if (legends.length === 0) return;
  const view = svg.viewBox.baseVal;
  const marks = Array.from(svg.querySelectorAll<SVGGraphicsElement>(".cw-plot-hover-target, .cw-plot-label.is-pinned"))
    .map(measuredElementBox)
    .filter((box): box is MeasuredLayoutBox => box !== null);
  const placed: MeasuredLayoutBox[] = [];
  let railX = view.x + 10;
  let railY = view.y + view.height + 12;
  for (const legend of legends) {
    const original = measuredElementBox(legend);
    if (!original) continue;
    const candidatePositions = [
      { x: view.x + view.width - original.width - 10, y: view.y + 10, name: "top-right" },
      { x: view.x + 10, y: view.y + 10, name: "top-left" },
      { x: view.x + view.width - original.width - 10, y: view.y + view.height - original.height - 10, name: "bottom-right" },
      { x: view.x + 10, y: view.y + view.height - original.height - 10, name: "bottom-left" },
    ];
    const scored = candidatePositions.map((candidate) => ({
      ...candidate,
      score: scoreLayoutCandidate(
        { ...original, x: candidate.x, y: candidate.y },
        [...marks, ...placed],
        { x: view.x, y: view.y, width: view.width, height: view.height },
      ),
    })).sort((left, right) => left.score - right.score);
    const best = scored[0]!;
    const external = best.score >= 10_000;
    const targetX = external ? railX : best.x;
    const targetY = external ? railY : best.y;
    legend.setAttribute("transform", `translate(${targetX - original.x} ${targetY - original.y})`);
    legend.dataset.layoutPosition = external ? "external-rail" : best.name;
    const finalBox = { ...original, x: targetX, y: targetY };
    placed.push(finalBox);
    if (external) {
      railX += original.width + 18;
      if (railX + original.width > view.x + view.width) {
        railX = view.x + 10;
        railY += original.height + 12;
      }
      extent.bottom = Math.max(extent.bottom, targetY + original.height);
    }
  }
}

function resolveMountedAnnotationCollisions(
  svg: SVGSVGElement,
  extent: { right: number; bottom: number },
): void {
  const view = svg.viewBox.baseVal;
  const labels = Array.from(svg.querySelectorAll<SVGTextElement>(
    ".cw-plot-label, .cw-plot-threshold-label",
  )).filter((label) => !label.closest(".cw-plot-composition-entry"));
  labels.sort((left, right) => Number(right.classList.contains("is-pinned")) - Number(left.classList.contains("is-pinned")));
  const obstacles = Array.from(svg.querySelectorAll<SVGGraphicsElement>(
    ".cw-plot-legend, .cw-plot-density-legend, .cw-plot-witness-legend, .cw-plot-dispersion-cumulative, .cw-plot-axis-title, .cw-plot-label.is-pinned",
  )).map(measuredElementBox).filter((box): box is MeasuredLayoutBox => box !== null);
  const placed: MeasuredLayoutBox[] = [];
  let railY = 18;
  for (const label of labels) {
    const original = measuredElementBox(label);
    if (!original || original.width === 0 || original.height === 0) continue;
    const relevantObstacles = [...obstacles.filter((box) => !sameBox(box, original)), ...placed];
    const candidates = annotationCandidates().map((candidate) => {
      const box = { ...original, x: original.x + candidate.dx, y: original.y + candidate.dy };
      return {
        candidate,
        box,
        score: scoreLayoutCandidate(box, relevantObstacles, {
          x: view.x + 4,
          y: view.y + 4,
          width: view.width - 8,
          height: view.height - 8,
        }, candidate.displacementPenalty),
      };
    }).sort((left, right) => left.score - right.score);
    const best = candidates[0]!;
    let dx = best.candidate.dx;
    let dy = best.candidate.dy;
    let finalBox = best.box;
    if (best.score >= 10_000) {
      const railX = view.x + view.width + 14;
      dx = railX - original.x;
      dy = railY - original.y;
      finalBox = { ...original, x: railX, y: railY };
      railY += original.height + 8;
      extent.right = Math.max(extent.right, railX + original.width);
      label.classList.add("is-external-rail");
    }
    if (dx !== 0 || dy !== 0) {
      label.setAttribute("transform", `translate(${dx} ${dy})`);
      if (Math.hypot(dx, dy) >= 24) appendMeasuredLeader(original, finalBox, label);
    }
    placed.push(finalBox);
    extent.bottom = Math.max(extent.bottom, finalBox.y + finalBox.height);
  }
}

function appendMeasuredLeader(
  original: MeasuredLayoutBox,
  target: MeasuredLayoutBox,
  before: SVGTextElement,
): void {
  const leader = document.createElementNS(SVG_NS, "line");
  leader.setAttribute("class", "cw-plot-label-leader is-layout-generated");
  leader.setAttribute("x1", String(original.x + original.width / 2));
  leader.setAttribute("y1", String(original.y + original.height / 2));
  leader.setAttribute("x2", String(target.x + (target.x > original.x ? 0 : target.width)));
  leader.setAttribute("y2", String(target.y + target.height / 2));
  before.parentNode?.insertBefore(leader, before);
}

function measuredTextWidth(text: SVGTextElement): number {
  try {
    return text.getComputedTextLength();
  } catch {
    return (text.textContent?.length ?? 0) * 6.5;
  }
}

function measuredElementBox(element: SVGGraphicsElement): MeasuredLayoutBox | null {
  try {
    const box = element.getBBox();
    const matrix = element.transform.baseVal.consolidate()?.matrix;
    if (!matrix) return { x: box.x, y: box.y, width: box.width, height: box.height };
    const points = [
      new DOMPoint(box.x, box.y).matrixTransform(matrix),
      new DOMPoint(box.x + box.width, box.y).matrixTransform(matrix),
      new DOMPoint(box.x, box.y + box.height).matrixTransform(matrix),
      new DOMPoint(box.x + box.width, box.y + box.height).matrixTransform(matrix),
    ];
    const left = Math.min(...points.map((point) => point.x));
    const top = Math.min(...points.map((point) => point.y));
    const right = Math.max(...points.map((point) => point.x));
    const bottom = Math.max(...points.map((point) => point.y));
    return { x: left, y: top, width: right - left, height: bottom - top };
  } catch {
    return null;
  }
}

function setTextPosition(text: SVGTextElement, x: number, y: number, anchor: "start" | "middle" | "end"): void {
  text.setAttribute("x", String(x));
  text.setAttribute("y", String(y));
  text.setAttribute("text-anchor", anchor);
}

function setCirclePosition(circle: SVGCircleElement, x: number, y: number): void {
  circle.setAttribute("cx", String(x));
  circle.setAttribute("cy", String(y));
}

function sameBox(left: MeasuredLayoutBox, right: MeasuredLayoutBox): boolean {
  return Math.abs(left.x - right.x) < 0.01
    && Math.abs(left.y - right.y) < 0.01
    && Math.abs(left.width - right.width) < 0.01
    && Math.abs(left.height - right.height) < 0.01;
}
