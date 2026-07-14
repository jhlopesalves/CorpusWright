import type { ProjectLanguage } from "../../generated/ProjectLanguage.js";

export const PROJECT_LANGUAGE_OPTIONS = ["en", "fr", "pt"] as const satisfies readonly ProjectLanguage[];
export const PROJECT_LANGUAGE_LABELS: Record<ProjectLanguage, string> = {
  en: "English",
  fr: "French",
  pt: "Portuguese",
};

export interface ProjectCreationMetadataDraft {
  name: string;
  language: string;
}

export function projectCreationMetadataIsValid(draft: ProjectCreationMetadataDraft): boolean {
  return draft.name.trim().length > 0 && projectLanguageFromValue(draft.language) !== null;
}

export function projectLanguageFromValue(value: string): ProjectLanguage | null {
  return PROJECT_LANGUAGE_OPTIONS.includes(value as ProjectLanguage)
    ? (value as ProjectLanguage)
    : null;
}

export function projectLanguageLabel(language: ProjectLanguage): string {
  return PROJECT_LANGUAGE_LABELS[language];
}

export function defaultProjectNameFromPath(path: string): string {
  const trimmed = path.trim().replace(/[\\/]+$/, "");
  const name = trimmed.split(/[\\/]/).filter(Boolean).pop();
  return name && name.trim().length > 0 ? name : "CorpusWright Project";
}

export function normaliseProjectDescription(value: string): string | null {
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}
