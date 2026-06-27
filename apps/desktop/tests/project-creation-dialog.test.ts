import {
  PROJECT_LANGUAGE_OPTIONS,
  defaultProjectNameFromPath,
  normaliseProjectDescription,
  projectCreationMetadataIsValid,
  projectLanguageFromValue,
  projectLanguageLabel,
} from "../src/project-creation-dialog.ts";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

assert(
  JSON.stringify(PROJECT_LANGUAGE_OPTIONS) === JSON.stringify(["en", "fr", "pt"]),
  "language select lists exactly en/fr/pt",
);
assert(projectLanguageLabel("en") === "English", "English label must be shown in full");
assert(projectLanguageLabel("fr") === "French", "French label must be shown in full");
assert(projectLanguageLabel("pt") === "Portuguese", "Portuguese label must be shown in full");
assert(projectLanguageFromValue("en") === "en", "English must be accepted");
assert(projectLanguageFromValue("fr") === "fr", "French must be accepted");
assert(projectLanguageFromValue("pt") === "pt", "Portuguese must be accepted");
assert(projectLanguageFromValue("de") === null, "unsupported languages must be rejected");

assert(
  !projectCreationMetadataIsValid({ name: "", language: "en" }),
  "new-project dialog blocks confirm until name is set",
);
assert(
  !projectCreationMetadataIsValid({ name: "   ", language: "en" }),
  "new-project dialog blocks confirm when name is blank after trimming",
);
assert(
  !projectCreationMetadataIsValid({ name: "Corpus", language: "" }),
  "new-project dialog blocks confirm until language is set",
);
assert(
  !projectCreationMetadataIsValid({ name: "Corpus", language: "de" }),
  "new-project dialog blocks confirm when language is unsupported",
);
assert(
  projectCreationMetadataIsValid({ name: "Corpus", language: "pt" }),
  "new-project dialog enables confirm when name and language are set",
);

assert(
  defaultProjectNameFromPath("C:\\corpora\\Interviews") === "Interviews",
  "folder name must prefill the project name",
);
assert(normaliseProjectDescription("  notes  ") === "notes", "description must be trimmed");
assert(normaliseProjectDescription("   ") === null, "blank description must be omitted");
