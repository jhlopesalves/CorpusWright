import {
  buildDispersionGeometry,
  buildDispersionPlaneGeometry,
  buildLexicalSpectrumGeometry,
} from "../../src/plots/model";
import {
  attachPlotHover,
  renderDispersionPlaneSvg,
  renderDispersionSvg,
  renderLexicalSpectrumSvg,
} from "../../src/plots/render";

const query = new URLSearchParams(location.search);
document.documentElement.dataset.theme = query.get("theme") ?? "corpus-light";
const root = document.querySelector<HTMLElement>("#fixture-root")!;

const profileCounts = [211_271, 14_610, 12_420, 10_210, 8_500, 7_200, 6_100, 5_000, 3_800, 0];
const profileTotal = profileCounts.reduce((sum, count) => sum + count, 0);
const profile = buildDispersionGeometry({
  buckets: profileCounts.map((count, index) => ({
    key: `band-${index}`,
    label: index === 0 ? "1 document" : index === 9 ? "All documents with an intentionally long category" : `${index + 1}–${index + 2} documents`,
    lowerBound: index + 1,
    upperBound: index + 1,
    count,
    share: count / profileTotal,
    sampleTypes: [`example-${index}`],
  })),
  totalTypes: profileTotal,
  totalDocuments: 100,
  cumulativeCallout: { count: 655, examples: ["the", "and", "of"] },
  pinnedKeys: new Set(["dispersion-reach-band-8"]),
});
mount("Dominant composition with narrow and zero bands", renderDispersionSvg("layout-profile", profile));

const lexical = buildLexicalSpectrumGeometry({
  points: [
    { frequency: 1, typeCount: 211_271 },
    { frequency: 2, typeCount: 41_999 },
    { frequency: 3, typeCount: 18_220 },
    { frequency: 9, typeCount: 2_101 },
    { frequency: 10, typeCount: 1_900 },
    { frequency: 100, typeCount: 220 },
    { frequency: 10_000, typeCount: 1 },
  ],
  hapaxCount: 211_271,
  disLegomenaCount: 41_999,
  band3To9Count: 37_000,
  band10plusCount: 21_500,
  totalTypes: 311_770,
  labelsMode: "authored",
  pinnedKeys: new Set(["spectrum:10000"]),
});
mount("Lexical class heading and edge-pin fixture", renderLexicalSpectrumSvg("layout-lexical", lexical));

const plane = buildDispersionPlaneGeometry({
  plane: {
    xBins: 12,
    yBins: 8,
    freqLog10Max: 6,
    counts: Array.from({ length: 96 }, (_, index) => index % 7 === 0 ? 0 : (index * 17) % 113),
    maxCellCount: 112,
    witnesses: [
      { token: "internationalisation", frequency: 420_000, documentFrequency: 98, dp: 0.08, kind: "core" },
      { token: "specialised-local-witness", frequency: 75_000, documentFrequency: 2, dp: 0.86, kind: "loudButLocal" },
      { token: "quiet-even", frequency: 43, documentFrequency: 80, dp: 0.14, kind: "quietAndEven" },
      { token: "rare-local", frequency: 2, documentFrequency: 1, dp: 0.93, kind: "rareAndLocal" },
    ],
    loudButLocalCount: 1,
    loudButLocalMinFrequency: 1_000,
    highDispersionThreshold: 0.5,
    reading: "fixture",
  },
  totalTypes: 5_000,
  totalTokens: 2_000_000,
  pinnedKeys: new Set(["plane:loudButLocal:specialised-local-witness"]),
});
mount("Plane legend occupancy fixture", renderDispersionPlaneSvg("layout-plane", plane));

function mount(title: string, svg: SVGSVGElement): void {
  const card = document.createElement("section");
  card.className = "layout-fixture-card";
  const heading = document.createElement("h1");
  heading.textContent = title;
  card.append(heading, svg);
  root.appendChild(card);
  attachPlotHover(svg);
}

void document.fonts.ready.then(() => requestAnimationFrame(() => requestAnimationFrame(validateGeometry)));

function validateGeometry(): void {
  const failures: string[] = [];
  for (const svg of root.querySelectorAll<SVGSVGElement>(".cw-plot-svg")) {
    const svgBounds = svg.getBoundingClientRect();
    const visibleTexts = Array.from(svg.querySelectorAll<SVGTextElement>("text")).filter((text) => {
      const style = getComputedStyle(text);
      return style.display !== "none" && style.visibility !== "hidden" && text.textContent?.trim();
    });
    for (const text of visibleTexts) {
      const box = transformedBox(text);
      if (!box) continue;
      if (box.left < svgBounds.left - 1 || box.top < svgBounds.top - 1 || box.right > svgBounds.right + 1 || box.bottom > svgBounds.bottom + 1) {
        failures.push(`${svg.dataset.plotId}: text outside viewBox: ${text.textContent}`);
      }
    }
    const mandatory = visibleTexts.filter((text) =>
      text.classList.contains("cw-plot-composition-label")
      || text.classList.contains("cw-plot-spectrum-band-label")
      || text.classList.contains("is-pinned")
    );
    for (let left = 0; left < mandatory.length; left += 1) {
      for (let right = left + 1; right < mandatory.length; right += 1) {
        const first = transformedBox(mandatory[left]!);
        const second = transformedBox(mandatory[right]!);
        if (mandatory[left]!.closest(".cw-plot-composition-entry") === mandatory[right]!.closest(".cw-plot-composition-entry")
          && mandatory[left]!.closest(".cw-plot-composition-entry")) continue;
        if (first && second && overlaps(first, second, 1)) {
          failures.push(`${svg.dataset.plotId}: mandatory text overlap: ${mandatory[left]!.textContent} / ${mandatory[right]!.textContent}`);
        }
      }
    }
  }
  const callout = root.querySelector<SVGGElement>(".cw-plot-dispersion-cumulative");
  const externalLabels = Array.from(root.querySelectorAll<SVGTextElement>(
    "[data-plot-id='layout-profile'] .cw-plot-composition-entry:is(.is-medium, .is-narrow, .is-zero) text",
  )).filter((text) => getComputedStyle(text).display !== "none");
  if (callout) {
    const calloutBox = transformedBox(callout);
    for (const label of externalLabels) {
      const labelBox = transformedBox(label);
      if (calloutBox && labelBox && overlaps(calloutBox, labelBox, 2)) failures.push("profile callout overlaps strip-label rail");
    }
  }
  const result = document.querySelector<HTMLElement>("#fixture-result")!;
  if (failures.length > 0) {
    document.body.dataset.visualStatus = "failed";
    result.textContent = `failed\n${failures.join("\n")}`;
    console.error(failures.join("\n"));
  } else {
    document.body.dataset.visualStatus = "passed";
    result.textContent = "passed";
  }
}

function transformedBox(element: SVGGraphicsElement): DOMRect | null {
  try {
    const box = element.getBoundingClientRect();
    return box.width > 0 && box.height > 0 ? box : null;
  } catch {
    return null;
  }
}

function overlaps(left: DOMRect, right: DOMRect, gap: number): boolean {
  return left.left < right.right + gap
    && left.right + gap > right.left
    && left.top < right.bottom + gap
    && left.bottom + gap > right.top;
}
