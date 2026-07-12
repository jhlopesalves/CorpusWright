declare global {
  interface Window {
    MathJax: Record<string, unknown>;
  }
}

window.MathJax = {
  tex: {
    inlineMath: [["\\(", "\\)"]],
    displayMath: [["\\[", "\\]"]]
  },
  options: { enableMenu: false }
};
