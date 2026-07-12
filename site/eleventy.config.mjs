export default function configureEleventy(eleventyConfig) {
  eleventyConfig.addPassthroughCopy({ "src/public": "." });

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
