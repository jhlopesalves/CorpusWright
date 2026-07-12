import path from "node:path";

export const stripComments = (html) => html.replace(/<!--[^]*?-->/g, "");

export function inspectHtml(html, { principal = true } = {}) {
  const source = stripComments(html);
  const errors = [];
  if (!/<html\b[^>]*\blang=["'][^"']+["']/i.test(source)) errors.push("missing document language");
  if (!/<title>\s*[^<\s][^<]*<\/title>/i.test(source)) errors.push("missing title");
  if (principal && !/<link\b[^>]*\brel=["']canonical["'][^>]*\bhref=["'][^"']+["']/i.test(source)) errors.push("missing canonical URL");
  const ids = [...source.matchAll(/\sid=["']([^"']+)["']/gi)].map((match) => match[1]);
  const seen = new Set();
  for (const id of ids) {
    if (seen.has(id)) errors.push(`duplicate ID: ${id}`);
    seen.add(id);
  }
  const headings = [...source.matchAll(/<h([1-6])\b/gi)].map((match) => Number(match[1]));
  if (principal && headings.filter((level) => level === 1).length !== 1) errors.push("principal page must contain exactly one h1");
  for (let index = 1; index < headings.length; index += 1) {
    if (headings[index] > headings[index - 1] + 1) errors.push(`skipped heading level: h${headings[index - 1]} to h${headings[index]}`);
  }
  if (/\sstyle=["']/i.test(source)) errors.push("inline style attribute");
  if (/<style\b/i.test(source)) errors.push("inline style block");
  for (const script of source.matchAll(/<script\b([^>]*)>/gi)) {
    const runtime = script[1].match(/\bsrc=["']([^"']+)["']/i)?.[1];
    if (!runtime) errors.push("inline behaviour script");
    else if (/^(?:https?:)?\/\//i.test(runtime)) errors.push(`remote executable script: ${runtime}`);
  }
  for (const image of source.matchAll(/<img\b[^>]*>/gi)) {
    if (!/\balt=["'][^"']*["']/i.test(image[0])) errors.push("image missing alt treatment");
  }
  for (const button of source.matchAll(/<button\b([^>]*)>([^]*?)<\/button>/gi)) {
    const label = button[1].match(/\baria-label=["']([^"']+)["']/i)?.[1] ?? button[2].replace(/<[^>]+>/g, "").trim();
    if (!label) errors.push("button missing accessible name");
  }
  for (const relationship of source.matchAll(/\saria-(?:controls|labelledby)=["']([^"']+)["']/gi)) {
    for (const id of relationship[1].split(/\s+/)) if (!seen.has(id)) errors.push(`broken ARIA relationship: ${id}`);
  }
  return errors;
}

export function inspectCss(css, { themeOwner = false } = {}) {
  const errors = [];
  if (!themeOwner && /#[0-9a-f]{3,8}\b|rgba?\(/i.test(css)) errors.push("raw CSS colour outside theme ownership");
  if (/font-weight:\s*(?!400\b|600\b)\d+/i.test(css)) errors.push("unsupported font weight");
  if (/@import\s+(?:url\()?['"]?https?:/i.test(css)) errors.push("remote CSS import");
  return errors;
}

function localPath(raw, sourceRoute) {
  const url = new URL(raw, `https://local.invalid${sourceRoute}`);
  if (!["local.invalid", "jhlopesalves.github.io"].includes(url.hostname)) return null;
  let pathname = decodeURIComponent(url.pathname);
  if (url.hostname === "jhlopesalves.github.io") pathname = pathname.replace(/^\/CorpusWright\/?/, "/");
  if (pathname.endsWith("/")) pathname += "index.html";
  return { pathname: pathname.replace(/^\//, ""), hash: url.hash ? decodeURIComponent(url.hash.slice(1)) : "" };
}

export function inspectLinks(documents, files) {
  const errors = [];
  let links = 0;
  let fragments = 0;
  const idsByFile = new Map([...documents].map(([route, html]) => [route.replace(/^\//, ""), new Set([...stripComments(html).matchAll(/\sid=["']([^"']+)["']/gi)].map((match) => match[1]))]));
  for (const [route, html] of documents) {
    const source = stripComments(html);
    const references = [
      ...source.matchAll(/<(?:a|link)\b[^>]*\bhref=["']([^"']+)["']/gi),
      ...source.matchAll(/<(?:img|script)\b[^>]*\bsrc=["']([^"']+)["']/gi)
    ];
    const refresh = [...source.matchAll(/<meta\b[^>]*http-equiv=["']refresh["'][^>]*content=["'][^"']*url=([^"';\s]+)[^"']*["']/gi)];
    for (const reference of [...references, ...refresh]) {
      const raw = reference[1].trim();
      if (!raw || /^(?:mailto:|tel:|data:|javascript:)/i.test(raw)) continue;
      links += 1;
      let target;
      try { target = localPath(raw, route); } catch { errors.push(`${route}: invalid URL ${raw}`); continue; }
      if (!target) continue;
      if (!files.has(target.pathname)) {
        errors.push(`${route}: missing target ${raw}`);
        continue;
      }
      if (target.hash) {
        fragments += 1;
        if (!idsByFile.get(target.pathname)?.has(target.hash)) errors.push(`${route}: missing fragment ${raw}`);
      }
    }
  }
  return { errors, links, fragments };
}

export function releaseRenderingErrors(html, release) {
  const errors = [];
  if (!html.includes(release.version)) errors.push("release version drift");
  if (!html.includes(release.dateReleased.slice(0, 4))) errors.push("release year drift");
  return errors;
}

export function missingManifestFiles(manifest, present) {
  return manifest.filter((file) => !present.has(path.posix.normalize(file)));
}
