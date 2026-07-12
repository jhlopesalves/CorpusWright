export default function configureEleventy(eleventyConfig) {
  eleventyConfig.addPassthroughCopy({ "src/public": "." });
  eleventyConfig.addWatchTarget("src/styles");
  eleventyConfig.addWatchTarget("src/scripts");
  eleventyConfig.addFilter("htmlText", (value) => String(value).trimEnd()
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;"));
  eleventyConfig.addFilter("braced", (value) => `{${value}}`);

  return {
    dir: {
      input: "src",
      output: "../docs",
      includes: "_includes",
      data: "_data"
    },
    templateFormats: ["njk"]
  };
}
