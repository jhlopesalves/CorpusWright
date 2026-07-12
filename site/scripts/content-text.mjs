import crypto from "node:crypto";

const entities = { amp: "&", gt: ">", lt: "<", quot: "\"", apos: "'", ldquo: "“", rdquo: "”" };

function decodeEntities(text) {
  return text.replace(/&(#x[0-9a-f]+|#\d+|[a-z][a-z0-9]+);/gi, (match, entity) => {
    if (entity[0] !== "#") return entities[entity] ?? match;
    return String.fromCodePoint(entity[1].toLowerCase() === "x" ? Number.parseInt(entity.slice(2), 16) : Number.parseInt(entity.slice(1), 10));
  });
}

export function normalisePublicText(html, dynamicValues) {
  let text = decodeEntities(html
    .replace(/<!--[^]*?-->/g, " ")
    .replace(/<(script|style)\b[^>]*>[^]*?<\/\1>/gi, " ")
    .replace(/<[^>]+>/g, " "))
    .replace(/\s+/g, " ")
    .trim();
  for (const [value, placeholder] of dynamicValues) text = text.split(value).join(placeholder);
  return [text];
}

export function textHash(text) {
  return crypto.createHash("sha256").update(JSON.stringify(text)).digest("hex");
}
