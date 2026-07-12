const storedTheme = (() => {
  try {
    return window.localStorage.getItem("theme");
  } catch {
    return null;
  }
})();
const systemTheme = window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
document.documentElement.dataset.theme = storedTheme === "dark" || storedTheme === "light" ? storedTheme : systemTheme;
