import type { ProjectLanguage } from "../../../generated/ProjectLanguage.js";

import englishStopwords from "../../../../../../crates/corpuswright-core/resources/stopwords/en.txt?raw";
import frenchStopwords from "../../../../../../crates/corpuswright-core/resources/stopwords/fr.txt?raw";
import portugueseStopwords from "../../../../../../crates/corpuswright-core/resources/stopwords/pt.txt?raw";

const stopwordSources: Record<ProjectLanguage, string> = {
  en: englishStopwords,
  fr: frenchStopwords,
  pt: portugueseStopwords,
};

const stopwordsByLanguage: Record<ProjectLanguage, ReadonlySet<string>> = {
  en: parseStopwords(stopwordSources.en),
  fr: parseStopwords(stopwordSources.fr),
  pt: parseStopwords(stopwordSources.pt),
};

export function isOverviewStopword(token: string, language: ProjectLanguage | null): boolean {
  return stopwordsByLanguage[language ?? "en"].has(token.toLocaleLowerCase());
}

function parseStopwords(source: string): ReadonlySet<string> {
  return new Set(source
    .split(/\r?\n/u)
    .map((word) => word.trim())
    .filter((word) => word.length > 0 && !word.startsWith("#"))
    .map((word) => word.toLocaleLowerCase()));
}
