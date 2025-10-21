export function escapeHtml(unsafe: string): string {
  const map: Record<string, string> = {};
  map["&"] = String.fromCharCode(38, 97, 109, 112, 59);
  map["<"] = String.fromCharCode(38, 108, 116, 59);
  map[">"] = String.fromCharCode(38, 103, 116, 59);
  map['"'] = String.fromCharCode(38, 113, 117, 111, 116, 59);
  map["'"] = String.fromCharCode(38, 35, 48, 51, 57, 59);
  return unsafe.replace(/[&<>"']/g, (ch) => map[ch] || ch);
}

export function highlightPreviewText(text: string, query: string): string {
  const lines = text.split("\n");
  let outHtml = "";

  const processStr = (s: string) => {
    if (!query) return escapeHtml(s);
    const lowerS = s.toLowerCase();
    const lowerQ = query.toLowerCase();
    let idx = 0;
    let newHtml = "";
    while (true) {
      const found = lowerS.indexOf(lowerQ, idx);
      if (found === -1) {
        newHtml += escapeHtml(s.substring(idx));
        break;
      }
      newHtml += escapeHtml(s.substring(idx, found));
      newHtml += `<mark class="search-match">${escapeHtml(s.substring(found, found + lowerQ.length))}</mark>`;
      idx = found + lowerQ.length;
    }
    return newHtml;
  };

  for (let i = 0; i < lines.length; i++) {
    outHtml += `${processStr(lines[i])}${i < lines.length - 1 ? "\n" : ""}`;
  }

  return outHtml;
}

export function sanitizeFolderName(name: string): string {
  return name.replace(/[<>:"/\\|?*\x00-\x1f]/g, "_")
    .replace(/\.+$/, "")
    .trim() || "CorpusWright";
}
