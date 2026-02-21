import type { ProjectLanguage } from "./ProjectLanguage.js";

export type CreateCorpusProjectRequest = { rootPath: string, name: string, language: ProjectLanguage, description: string | null, };
