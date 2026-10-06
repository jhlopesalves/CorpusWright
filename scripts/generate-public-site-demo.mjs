/**
 * Generate the controlled statistical demonstration corpus, outside the repository.
 * Analyse it with `cargo xtask documentation-dossier`; this script calculates no measures.
 */
import { mkdirSync, readdirSync, realpathSync, writeFileSync } from "node:fs";
import { resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const argument = process.argv[2];
if (!argument || process.argv.length !== 3) {
  throw new Error("Usage: node scripts/generate-public-site-demo.mjs <empty-output-directory>");
}
const output = resolve(argument);
const repository = resolve(fileURLToPath(new URL("..", import.meta.url)));
if (output === repository || output.startsWith(repository + sep)) {
  throw new Error("Generate the demonstration corpus outside the repository.");
}
mkdirSync(output, { recursive: true });
// A path outside the checkout can still point into it through an existing symlink.
const physicalRepository = realpathSync(repository);
const physicalOutput = realpathSync(output);
if (physicalOutput === physicalRepository || physicalOutput.startsWith(physicalRepository + sep)) {
  throw new Error("Generate the demonstration corpus outside the repository.");
}
if (readdirSync(output).length !== 0) {
  throw new Error("The demonstration output directory must be empty.");
}

// Interpretable synthetic vocabulary; compound labels are controlled identifiers,
// not a claim that these texts reproduce natural-language word distributions.
const sharedWords = `compost acorn alder apple apricot asparagus basil bean beech birch
blackberry blossom blueberry borage branch broccoli bud cabbage calendula carrot
cauliflower celery cherry chive clover coriander courgette cucumber daffodil dahlia
root fennel fig flax flower garlic gooseberry grape grass hazel hedge holly iris kale
lavender leek lemon lettuce lime marigold mint moss nettle oak onion orchid oregano
pansy parsnip peach pear pepper plum poppy pumpkin radish raspberry rhubarb rosemary
sage spinach squash stem strawberry sunflower thyme tomato tulip turnip walnut`.split(/\s+/);
const topicWords = `artichoke beetroot chamomile endive foxglove hyacinth juniper kohlrabi
lupin mulberry nasturtium sorrel`.split(/\s+/);
const adjectives = `amber azure bronze coral crimson dusty emerald golden grey indigo
ivory jade lilac mauve ochre olive orange pale pink purple red russet silver slate
soft tawny teal violet white yellow`.split(/\s+/);
const plants = `anemone aster bamboo bay bracken camellia cedar clematis cypress daisy
elm fern fuchsia gentian geranium heather hibiscus ivy jasmine larch laurel lily
magnolia maple mimosa myrtle palm peony petunia pine primrose rose sedge spruce
thistle verbena vine willow wisteria yew`.split(/\s+/);
const tools = `basket bench bucket can cart cover fork handle marker pot rake shovel`.split(/\s+/);
const compound = (index, nouns) => adjectives[Math.floor(index / nouns.length)] + nouns[index % nouns.length];
if (sharedWords.length !== 80 || topicWords.length !== 12 || adjectives.length * plants.length !== 1200 || adjectives.length * tools.length !== 360) {
  throw new Error("Controlled vocabulary dimensions changed.");
}
const vocabulary = ["the", "and", "of", "in", "a", "leaves", "garden", "soil", "watering", "seedling",
  ...sharedWords, ...topicWords,
  ...Array.from({ length: 1200 }, (_, index) => compound(index, plants)),
  ...Array.from({ length: 360 }, (_, index) => compound(index, tools))];
if (new Set(vocabulary).size !== 1662) throw new Error("Controlled type mapping must remain one-to-one.");

let randomState = 20261005;
function shuffle(tokens) {
  // A fixed integer generator keeps bytes, token order and growth checkpoints reproducible.
  for (let index = tokens.length - 1; index > 0; index -= 1) {
    randomState = (Math.imul(1664525, randomState) + 1013904223) >>> 0;
    const selected = randomState % (index + 1);
    [tokens[index], tokens[selected]] = [tokens[selected], tokens[index]];
  }
}

for (let document = 0; document < 12; document += 1) {
  const tokens = [];
  const append = (token, count) => {
    for (let index = 0; index < count; index += 1) tokens.push(token);
  };
  for (const [token, count] of [
    ["the", 600], ["and", 300], ["of", 200], ["in", 150], ["a", 100],
    ["leaves", 60], ["garden", 48], ["soil", 36],
  ]) append(token, count);
  for (let rank = 0; rank < 80; rank += 1) append(sharedWords[rank], Math.ceil(120 / (rank + 1)));
  for (let rare = 0; rare < 100; rare += 1) append(compound(document * 100 + rare, plants), 1);
  for (let rare = 0; rare < 30; rare += 1) append(compound(document * 30 + rare, tools), 2);
  append(topicWords[document], 140);
  append("watering", 100);
  if (document === 0) append("seedling", 1200);
  if (tokens.length > 4096) throw new Error("Demonstration document exceeds its fixed token budget.");
  // Equal document lengths isolate dispersion from document-size imbalance in this example.
  append("the", 4096 - tokens.length);
  shuffle(tokens);
  const lines = [];
  for (let start = 0; start < tokens.length; start += 32) lines.push(tokens.slice(start, start + 32).join(" "));
  writeFileSync(resolve(output, `document-${String(document + 1).padStart(2, "0")}.txt`), lines.join("\n") + "\n");
}
process.stdout.write("Generated the controlled statistical demonstration corpus: twelve synthetic, equal-length documents.\n");
