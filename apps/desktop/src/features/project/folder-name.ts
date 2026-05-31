export function sanitizeFolderName(name: string): string {
  return name.replace(/[<>:"/\\|?*\x00-\x1f]/g, "_")
    .replace(/\.+$/, "")
    .trim() || "CorpusWright";
}
