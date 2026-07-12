const commonDescription = "CorpusWright: a local-first corpus linguistics workbench for cleaning, inspecting, and analysing text corpora.";
const commonFooter = {
  footerText: "CorpusWright is an open, local-first corpus linguistics workbench, currently in public alpha.",
  footerLinkText: "View the source on GitHub"
};

export default {
  home: {
    title: "CorpusWright - local-first corpus workbench",
    description: "CorpusWright is a local-first workbench for preparing, describing, and exploring a corpus.",
    openGraphDescription: "CorpusWright is a local-first workbench for preparing, describing, and exploring a corpus.",
    canonicalUrl: "https://jhlopesalves.github.io/CorpusWright/",
    assetPrefix: "",
    navigation: null,
    footerText: "CorpusWright is an open, local-first corpus linguistics tool, currently in public alpha.",
    footerLinkText: "Source on GitHub",
    screenshotTabs: true
  },
  quickStart: {
    title: "Quick start · CorpusWright",
    description: commonDescription,
    openGraphDescription: commonDescription,
    canonicalUrl: "https://jhlopesalves.github.io/CorpusWright/quick-start.html",
    assetPrefix: "",
    navigation: "quickStart",
    ...commonFooter
  },
  concepts: {
    title: "Concepts - Overview - CorpusWright",
    description: "Concepts overview: how to read the statistics CorpusWright reports, from dispersion and lexical diversity to Zipf, Heaps, and entropy.",
    openGraphDescription: "Concepts overview: how to read the statistics CorpusWright reports.",
    canonicalUrl: "https://jhlopesalves.github.io/CorpusWright/concepts/overview.html",
    assetPrefix: "../",
    navigation: "concepts",
    conceptsRuntime: true,
    ...commonFooter
  },
  roadmap: {
    title: "Roadmap · CorpusWright",
    description: commonDescription,
    openGraphDescription: commonDescription,
    canonicalUrl: "https://jhlopesalves.github.io/CorpusWright/roadmap.html",
    assetPrefix: "",
    navigation: "roadmap",
    ...commonFooter
  },
  technicalDocs: {
    title: "Technical docs · CorpusWright",
    description: commonDescription,
    openGraphDescription: commonDescription,
    canonicalUrl: "https://jhlopesalves.github.io/CorpusWright/technical-docs.html",
    assetPrefix: "",
    navigation: "technicalDocs",
    ...commonFooter
  },
  citation: {
    title: "Citation · CorpusWright",
    description: "Citation guidelines for the CorpusWright corpus linguistics software.",
    openGraphDescription: "Citation guidelines for the CorpusWright corpus linguistics software.",
    canonicalUrl: "https://jhlopesalves.github.io/CorpusWright/citation.html",
    assetPrefix: "",
    navigation: null,
    ...commonFooter
  },
  about: {
    title: "About · CorpusWright",
    description: "About CorpusWright: where the project stands, and who maintains it.",
    openGraphDescription: "About CorpusWright: where the project stands, and who maintains it.",
    canonicalUrl: "https://jhlopesalves.github.io/CorpusWright/about.html",
    assetPrefix: "",
    navigation: "about",
    ...commonFooter
  },
  notFound: {
    title: "Page not found · CorpusWright",
    description: commonDescription,
    openGraphDescription: commonDescription,
    canonicalUrl: "https://jhlopesalves.github.io/CorpusWright/404.html",
    assetPrefix: "",
    navigation: null,
    ...commonFooter
  }
};
