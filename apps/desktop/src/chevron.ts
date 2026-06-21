const SVG_NS = "http://www.w3.org/2000/svg";

export function createChevron(className?: string): SVGSVGElement {
  const icon = document.createElementNS(SVG_NS, "svg");
  icon.classList.add("cw-chevron");
  if (className) {
    icon.classList.add(className);
  }
  icon.setAttribute("viewBox", "0 0 16 16");
  icon.setAttribute("width", "14");
  icon.setAttribute("height", "14");
  icon.setAttribute("fill", "none");
  icon.setAttribute("stroke", "currentColor");
  icon.setAttribute("stroke-width", "2");
  icon.setAttribute("stroke-linecap", "round");
  icon.setAttribute("stroke-linejoin", "round");
  icon.setAttribute("aria-hidden", "true");
  icon.setAttribute("focusable", "false");

  const path = document.createElementNS(SVG_NS, "path");
  path.setAttribute("d", "M5.5 3.5L10 8l-4.5 4.5");
  icon.appendChild(path);
  return icon;
}
