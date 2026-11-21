/** Browser acceptance for the Wright shell, reading pages and scroll-driven walkthrough. */
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { extname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { browserFailures, createCapturePage, launchCaptureBrowser, waitFor } from "../apps/desktop/scripts/browser-capture.mjs";

const docs = fileURLToPath(new URL("../docs", import.meta.url));
const output = process.argv[2] ?? mkdtempSync(join(tmpdir(), "corpuswright-public-site-qa-"));
mkdirSync(output, { recursive: true });
const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".png": "image/png", ".svg": "image/svg+xml", ".woff2": "font/woff2" };
const server = createServer((request, response) => {
  const pathname = decodeURIComponent(new URL(request.url, "http://site.local").pathname);
  const path = resolve(docs, `.${pathname === "/" ? "/index.html" : pathname}`);
  try {
    assert.ok(path.startsWith(docs + sep));
    response.setHeader("Content-Type", types[extname(path)] ?? "application/octet-stream");
    response.end(readFileSync(path));
  } catch {
    response.writeHead(404);
    response.end("Not found");
  }
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
const base = `http://127.0.0.1:${server.address().port}`;
const browser = await launchCaptureBrowser();
const cases = [];
const widths = [1440, 1280, 1024, 1000, 999, 900, 880, 840, 820, 800, 768, 740, 721, 720, 390, 375, 320];
const stages = ["sources", "artefacts", "cleaning", "counts", "dispersion", "context", "record"];
const evidenceWidths = [1440, 900, 390];
const readingPages = ["quick-start.html", "concepts/overview.html", "about.html", "citation.html", "technical-docs.html", "roadmap.html", "404.html"];

function contrastRatio(foreground, background) {
  const luminance = color => {
    const values = (Array.isArray(color) ? color : color.match(/[\d.]+/g).slice(0, 3).map(Number)).map(value => {
      const channel = value / 255;
      return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
    });
    return values[0] * 0.2126 + values[1] * 0.7152 + values[2] * 0.0722;
  };
  const values = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  return (values[0] + 0.05) / (values[1] + 0.05);
}

async function checkUIContrast(page, label) {
  const samples = await page.evaluate(`(() => {
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = 1;
    const pixel = canvas.getContext('2d', { willReadFrequently: true });
    const bodyColour = getComputedStyle(document.body).backgroundColor;
    const hero = document.querySelector('.walk-hero');
    const heroColour = hero && getComputedStyle(hero, '::before').backgroundColor;
    const paint = colours => {
      pixel.clearRect(0, 0, 1, 1);
      for (const colour of colours) {
        pixel.fillStyle = colour;
        pixel.fillRect(0, 0, 1, 1);
      }
      return [...pixel.getImageData(0, 0, 1, 1).data].slice(0, 3);
    };
    return [...document.querySelectorAll('.site-header .brand, .nav-links a, .theme-btn, .btn, .site-footer nav a, .walk-status a, .walk-full-image')]
      .filter(element => element.getClientRects().length)
      .map(element => {
        const overHero = hero && element.closest('.walk-hero, .site-header:not(.is-solid)');
        const backdrop = [overHero ? heroColour : bodyColour];
        const layers = [];
        for (let ancestor = element; ancestor !== document.body; ancestor = ancestor.parentElement) {
          layers.unshift(getComputedStyle(ancestor).backgroundColor);
        }
        backdrop.push(...layers);
        return { name: element.textContent.trim() || element.getAttribute('aria-label'),
          foreground: paint([...backdrop, getComputedStyle(element).color]), background: paint(backdrop) };
      });
  })()`);
  for (const { name, foreground, background } of samples) {
    const contrast = contrastRatio(foreground, background);
    assert.ok(contrast >= 4.5, `${label}: ${name} UI contrast ${contrast.toFixed(2)}:1`);
  }
}

async function open(path, width, theme, { reducedMotion = false, javascript = true, zoom = 1, textScale = 1, resetTheme = true } = {}) {
  const page = await createCapturePage(browser, width / zoom, 1000 / zoom);
  if (zoom !== 1) {
    // A narrower CSS viewport at matching pixel density reproduces browser zoom's layout and pixels.
    await browser.send("Emulation.setDeviceMetricsOverride", {
      width: width / zoom, height: 1000 / zoom, deviceScaleFactor: zoom, mobile: false,
      screenWidth: width, screenHeight: 1000,
    }, page.sessionId);
  }
  if (resetTheme) await browser.send("Page.addScriptToEvaluateOnNewDocument", { source: "localStorage.removeItem('theme');" }, page.sessionId);
  await browser.send("Emulation.setEmulatedMedia", { features: [
    { name: "prefers-color-scheme", value: theme },
    { name: "prefers-reduced-motion", value: reducedMotion ? "reduce" : "no-preference" },
  ] }, page.sessionId);
  if (!javascript) await browser.send("Emulation.setScriptExecutionDisabled", { value: true }, page.sessionId);
  await page.navigate(base + path);
  await waitFor(page, "document.readyState === 'complete'");
  if (textScale !== 1) await page.evaluate(`document.documentElement.style.fontSize = '${textScale * 100}%'`);
  await page.evaluate("document.fonts.ready");
  if (path === "/concepts/overview.html") await checkNativeMethods(page);
  return page;
}

async function checkNativeMethods(page) {
  const state = await page.evaluate(`(() => ({
    displayFormulae: document.querySelectorAll('.math-display').length,
    displayedMath: [...document.querySelectorAll('.math-display')].filter(block =>
      [...block.querySelectorAll('math[display="block"]')].some(math => math.getBoundingClientRect().height > 0)).length,
    errors: document.querySelectorAll('merror').length,
    flows: [...document.querySelectorAll('ol.method-flow')].map(flow => {
      const disclosure = flow.closest('details');
      const closed = disclosure && !disclosure.open;
      if (closed) disclosure.open = true;
      const result = { steps: flow.querySelectorAll(':scope > li').length, visible: flow.getBoundingClientRect().height > 0 };
      if (closed) disclosure.open = false;
      return result;
    }),
  }))()`);
  assert.ok(state.displayFormulae > 0, "Methods must contain display formulae");
  // Closed native disclosures retain their MathML in the document without laying it out yet.
  const complete = await page.evaluate("[...document.querySelectorAll('.math-display')].every(block => block.querySelector('math[display=block]'))");
  assert.equal(complete, true, "Every display formula must use native MathML");
  assert.ok(state.displayedMath > 0, "Native MathML must render");
  assert.equal(state.errors, 0, "Methods formula parse errors");
  assert.equal(state.flows.length, 3, "Methods must keep its three process diagrams");
  assert.ok(state.flows.every(flow => flow.visible && flow.steps >= 2), "Methods process steps must be readable");
}

async function checkPage(page, label) {
  const failures = browserFailures(browser, page.sessionId);
  assert.deepEqual(failures, [], `${label}: browser errors`);
  const network = browser.events.filter(event => event.sessionId === page.sessionId && event.method === "Network.requestWillBeSent")
    .map(event => new URL(event.params.request.url)).filter(url => /^https?:$/.test(url.protocol));
  assert.deepEqual([...new Set(network.filter(url => url.origin !== base).map(url => url.href))], [], `${label}: third-party network requests`);
  const geometry = await page.evaluate("({ width: innerWidth, content: document.documentElement.scrollWidth, font: getComputedStyle(document.querySelector('h1')).fontFamily })");
  assert.ok(geometry.content <= geometry.width, `${label}: horizontal overflow ${JSON.stringify(geometry)}`);
  const splitNumbers = await page.evaluate(`(() => [...document.querySelectorAll('.worked-example td')]
    .filter(cell => /^\\d[\\d,.]*$/.test(cell.textContent.trim()))
    .filter(cell => {
      const range = document.createRange();
      range.selectNodeContents(cell);
      return range.getClientRects().length !== 1;
    }).map(cell => cell.textContent.trim()))()`);
  assert.deepEqual(splitNumbers, [], `${label}: worked-example numbers must stay on one line`);
  const overflowingFormulae = await page.evaluate(`(() => {
    const sectionIds = ['document-size-concentration', 'the-frequency-distribution-zipfs-law'];
    return sectionIds.flatMap(id => [...(document.getElementById(id)?.closest('section').querySelectorAll('.math-display') ?? [])]
      .filter(element => element.scrollWidth > element.clientWidth + 1).map(() => id));
  })()`);
  assert.deepEqual(overflowingFormulae, [], `${label}: document-size and rank formulae must fit the reading width`);
  return geometry;
}

async function checkFonts(page, label) {
  const fonts = await page.evaluate(`(async () => {
    const faces = [
      ['Manrope', '600', 'normal', 'CorpusWright'],
      ['Wright Reading', '400', 'normal', 'Frequency ⅓ ⅙'],
      ['Wright Reading', '400', 'italic', 'probability'],
      ['Wright Code', '400', 'normal', 'manifest.json'],
    ];
    return Promise.all(faces.map(async ([family, weight, style, text]) => {
      const loaded = await document.fonts.load(style + ' ' + weight + ' 16px "' + family + '"', text);
      return { family, style, loaded: loaded.some(face => face.status === 'loaded' && face.style === style
        && face.family.replaceAll('"', '') === family) };
    }));
  })()`);
  assert.ok(fonts.every(face => face.loaded), `${label}: Wright faces must load ${JSON.stringify(fonts)}`);
}

async function checkDesignSystem(page, label, textScale = 1) {
  const geometry = await checkPage(page, label);
  await checkFonts(page, label);
  const reading = await page.evaluate(`(() => {
    const home = document.body.classList.contains('home');
    const h1 = document.querySelector('h1');
    const h2 = document.querySelector('main h2:not(#docs-toc-title)');
    const paragraph = document.querySelector(home
      ? '.walk-hero .lede, .walk-hero h1 + p'
      : '.learn-section > p, .shell.narrow > p') ?? document.querySelector('main p');
    const body = getComputedStyle(document.body);
    const style = getComputedStyle(paragraph);
    const canvas = document.createElement('canvas').getContext('2d');
    canvas.font = style.font;
    const footer = document.querySelector('.site-footer .shell');
    const header = document.querySelector('.site-header');
    const intro = document.querySelector('.page-intro');
    const shell = header.querySelector('.shell');
    const brand = header.querySelector('.brand');
    const controls = [...header.querySelectorAll('.brand, .nav-links a, .theme-btn')];
    const font = element => getComputedStyle(element).fontFamily.split(',')[0].replaceAll('"', '');
    return {
      chromeFonts: [...document.querySelectorAll('main h1, main h2, main h3, .nav-links a, .btn')].map(font),
      proseFont: font(paragraph),
      codeFonts: [...document.querySelectorAll('main code, main pre')].map(font),
      weights: [...document.querySelectorAll('main h1, main h2, main h3')].filter(element => !element.closest('.docs-toc')).map(element => getComputedStyle(element).fontWeight),
      groupWeights: [...document.querySelectorAll('.toc-group h3')].map(element => getComputedStyle(element).fontWeight),
      fontSynthesis: body.fontSynthesis,
      role: home ? 'home' : document.body.classList.contains('methods') ? 'methods' : 'guide-about',
      headingLeading: [h1, h2].filter(Boolean).map(element => parseFloat(getComputedStyle(element).lineHeight) / parseFloat(getComputedStyle(element).fontSize)),
      targets: [...controls, ...document.querySelectorAll('.btn, .site-footer nav a, .walk-full-image')].filter(element => element.getClientRects().length)
        .map(element => ({ label: element.textContent.trim() || element.getAttribute('aria-label'), height: element.getBoundingClientRect().height, width: element.getBoundingClientRect().width })),
      themeBorder: getComputedStyle(document.querySelector('.theme-btn')).borderTopWidth,
      navigationRows: new Set([...header.querySelectorAll('.nav-links a')].map(element => Math.round(element.getBoundingClientRect().top))).size,
      headerHeight: header.getBoundingClientRect().height,
      headerControls: controls.filter(element => element.getClientRects().length).map(element => {
        const rect = element.getBoundingClientRect();
        return { label: element.textContent.trim() || element.getAttribute('aria-label'),
          left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom };
      }),
      guideFigures: [...document.querySelectorAll('.guide-figure')].map(element => ({ width: element.getBoundingClientRect().width, x: element.getBoundingClientRect().x })),
      productFigures: [...document.querySelectorAll('.product-figure')].map(element => ({ width: element.getBoundingClientRect().width })),
      axis: (home ? [brand, h1, paragraph] : [brand, h1, h2, paragraph]).filter(Boolean).map(element => element.getBoundingClientRect().x),
      footerAxis: footer.getBoundingClientRect().x + parseFloat(getComputedStyle(footer).paddingLeft),
      h1Size: parseFloat(getComputedStyle(h1).fontSize),
      h2Size: h2 ? parseFloat(getComputedStyle(h2).fontSize) : null,
      bodySize: parseFloat(body.fontSize), leading: parseFloat(body.lineHeight) / parseFloat(body.fontSize),
      proseColor: style.color, primaryColor: body.color,
      backgroundColor: home ? getComputedStyle(document.querySelector('.walk-hero'), '::before').backgroundColor : body.backgroundColor,
      proseCh: paragraph.getBoundingClientRect().width / canvas.measureText('0').width,
      stepProse: [...document.querySelectorAll('.walk-step > p:not(.walk-step-label)')].map(element => {
        const prose = getComputedStyle(element);
        canvas.font = prose.font;
        return { colour: prose.color, background: body.backgroundColor,
          measure: element.getBoundingClientRect().width / canvas.measureText('0').width };
      }),
      shellWidth: shell.getBoundingClientRect().width,
      introPadding: intro ? [getComputedStyle(intro).paddingTop, getComputedStyle(intro).paddingBottom].map(parseFloat) : null,
      eyebrows: document.querySelectorAll('.page-intro .eyebrow').length,
      tocPosition: document.querySelector('.docs-toc') ? getComputedStyle(document.querySelector('.docs-toc')).position : null,
    };
  })()`);
  const isHome = reading.role === "home";
  assert.ok(reading.chromeFonts.every(font => font === "Manrope"), `${label}: Wright chrome face ${[...new Set(reading.chromeFonts)]}`);
  assert.equal(reading.proseFont, "Wright Reading", `${label}: Wright reading face`);
  assert.ok(reading.codeFonts.every(font => font === "Wright Code"), `${label}: Wright code face ${[...new Set(reading.codeFonts)]}`);
  assert.ok(reading.weights.every(weight => Number(weight) >= 600 && Number(weight) <= 800), `${label}: heading weights`);
  assert.ok(reading.groupWeights.every(weight => weight === "500"), `${label}: TOC group weights`);
  assert.equal(reading.fontSynthesis, "none", `${label}: font styles must use real faces`);
  const smallTargets = reading.targets.filter(target => target.height < 44 || target.width < 44);
  assert.deepEqual(smallTargets, [], `${label}: controls need 44px targets`);
  assert.equal(reading.themeBorder, "0px", `${label}: theme utility must not compete with Download`);
  assert.ok(reading.headingLeading.every(ratio => ratio >= 1 && ratio <= 1.3), `${label}: compact heading leading`);
  assert.ok(reading.axis.every(axis => Math.abs(axis - reading.axis[0]) < 1), `${label}: shared left axis ${reading.axis}`);
  assert.ok(Math.abs(reading.footerAxis - reading.axis[0]) < 1, `${label}: footer axis`);
  assert.ok(reading.proseCh <= 72.1, `${label}: prose measure ${reading.proseCh}ch`);
  if (!isHome) {
    assert.equal(reading.proseColor, reading.primaryColor, `${label}: primary prose must use the primary text colour`);
    const expectedLeading = reading.role === "methods" ? 1.64 : 1.58;
    assert.ok(Math.abs(reading.leading - expectedLeading) < 0.005, `${label}: ${reading.role} body leading`);
  }
  reading.proseContrast = contrastRatio(reading.proseColor, reading.backgroundColor);
  assert.ok(reading.proseContrast >= 7, `${label}: primary prose contrast ${reading.proseContrast}`);
  for (const prose of reading.stepProse) {
    assert.ok(prose.measure <= 72.1, `${label}: step prose measure ${prose.measure}ch`);
    assert.ok(contrastRatio(prose.colour, prose.background) >= 7, `${label}: step prose contrast`);
  }
  assert.equal(reading.eyebrows, 0, `${label}: redundant page eyebrow`);
  if (textScale === 1) {
    if (isHome) {
      assert.ok(reading.h1Size >= 38 && reading.h1Size <= 60, `${label}: display title scale`);
    } else {
      assert.ok(reading.h1Size >= 32 && reading.h1Size <= 44, `${label}: title scale`);
      assert.ok(reading.h2Size >= 24 && reading.h2Size <= 28, `${label}: section scale`);
    }
    assert.ok(reading.bodySize >= 16 && reading.bodySize <= 17, `${label}: body scale`);
    assert.ok(!reading.introPadding || reading.introPadding.every(padding => padding <= 32), `${label}: compact page introduction`);
    assert.equal(reading.navigationRows, 1, `${label}: navigation items must each fit on the same row`);
    if (geometry.width >= 1280) {
      assert.equal(reading.shellWidth, isHome ? Math.min(geometry.width, 1440) : 1180, `${label}: documented shell width`);
      assert.ok(reading.headerHeight >= 68 && reading.headerHeight <= 76, `${label}: compact desktop header`);
    }
    if (geometry.width <= 390) {
      assert.ok(reading.headerHeight >= 88 && reading.headerHeight <= 128, `${label}: two-row phone header`);
    }
    if (!isHome && geometry.width >= 1024) assert.ok(reading.proseCh >= 60, `${label}: prose unnecessarily narrow`);
    for (const figure of reading.guideFigures) {
      assert.ok(Math.abs(figure.x - reading.axis[0]) < 1, `${label}: Guide image left axis`);
      if (geometry.width >= 1280) assert.equal(figure.width, 960, `${label}: Guide image breakout`);
    }
    for (const figure of reading.productFigures) {
      assert.ok(figure.width <= 800, `${label}: controlled Methods figure width`);
      if (geometry.width >= 1280) assert.equal(figure.width, 800, `${label}: dense Methods figure breakout`);
    }
  } else {
    assert.ok(reading.bodySize >= 32, `${label}: text enlargement actually applied`);
    if (reading.tocPosition) {
      assert.equal(reading.tocPosition, "static", `${label}: enlarged-text TOC reflow`);
      assert.ok(reading.proseCh >= 45, `${label}: enlarged Methods prose unnecessarily narrow`);
    }
  }
  for (let index = 0; index < reading.headerControls.length; index++) {
    const control = reading.headerControls[index];
    assert.ok(control.left >= -0.5 && control.right <= geometry.width + 0.5, `${label}: header control outside viewport: ${control.label}`);
    for (const other of reading.headerControls.slice(index + 1)) {
      const overlap = control.left < other.right - 0.5 && other.left < control.right - 0.5
        && control.top < other.bottom - 0.5 && other.top < control.bottom - 0.5;
      assert.equal(overlap, false, `${label}: header controls overlap: ${control.label} and ${other.label}`);
    }
  }
  return { ...geometry, ...reading, chromeFonts: undefined, codeFonts: undefined, weights: undefined };
}

async function checkFigureRequests(page, label, theme) {
  const requests = [...new Set(browser.events.filter(event => event.sessionId === page.sessionId && event.method === "Network.requestWillBeSent")
    .map(event => event.params.request.url).filter(url => url.includes("/assets/figures/")))];
  const opposite = theme === "light" ? "dark" : "light";
  assert.ok(requests.every(url => !url.endsWith(`-${opposite}.svg`)), `${label}: hidden theme figures must not be fetched`);
  return requests.length;
}

async function checkToc(page, label) {
  const state = await page.evaluate(`(() => {
    const toc = document.querySelector('.docs-toc'), prose = document.querySelector('.docs-content');
    const bounds = toc.getBoundingClientRect(), content = prose.getBoundingClientRect();
    return { links: toc.querySelectorAll('a[href^="#"]').length, x: bounds.x, width: bounds.width,
      contentRight: content.right, contentWidth: content.width, side: getComputedStyle(toc).position === 'sticky' };
  })()`);
  assert.equal(state.links, 16, `${label}: Methods TOC preserved`);
  assert.deepEqual(await page.evaluate("[...document.querySelectorAll('.docs-toc .toc-group h3')].map(node => node.textContent)"), ["Foundations", "Counts and distribution", "Vocabulary", "Design and interpretation"], `${label}: grouped single-page navigation`);
  if (state.side) {
    assert.ok(state.x > state.contentRight && state.width <= 224, `${label}: compact right-side TOC`);
    assert.ok(state.contentWidth > 600, `${label}: Methods prose width`);
  }
  await page.evaluate("document.querySelector('.docs-toc a[href=\"#dispersion-how-evenly-a-word-is-spread\"]').click()");
  // The click marks its link immediately; wait for smooth scrolling to reach
  // the section before exercising a real return from it to the page top.
  await waitFor(page, "(() => { const heading = document.getElementById('dispersion-how-evenly-a-word-is-spread'); return window.scrollY > 0 && Math.abs(heading.getBoundingClientRect().top - parseFloat(getComputedStyle(heading).scrollMarginTop)) <= 1; })()");
  await waitFor(page, "document.querySelector('.docs-toc a[aria-current=true]')?.hash === '#dispersion-how-evenly-a-word-is-spread'");
  const active = await page.evaluate("({ count: document.querySelectorAll('.docs-toc a[aria-current=true]').length, weight: getComputedStyle(document.querySelector('.docs-toc a[aria-current=true]')).fontWeight })");
  assert.equal(active.count, 1);
  assert.equal(active.weight, "600");
  await page.evaluate("window.scrollTo({ top: 0, behavior: 'instant' })");
  await waitFor(page, "document.querySelector('.docs-toc a[aria-current=true]')?.hash === '#how-to-read-this-section'");
  return state;
}

async function checkKeyboard(page, label) {
  await page.evaluate("document.querySelector('.skip-link').focus()");
  await browser.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter" }, page.sessionId);
  await browser.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter" }, page.sessionId);
  assert.equal(await page.evaluate("document.activeElement.id"), "top", `${label}: keyboard skip destination`);
  const before = await page.evaluate("document.documentElement.dataset.theme");
  await page.evaluate("document.querySelector('#theme-toggle').focus()");
  for (let step = 0; step < 2; step++) {
    await browser.send("Input.dispatchKeyEvent", { type: "keyDown", key: " ", code: "Space" }, page.sessionId);
    await browser.send("Input.dispatchKeyEvent", { type: "keyUp", key: " ", code: "Space" }, page.sessionId);
    assert.equal(await page.evaluate("document.documentElement.dataset.theme"), step === 0 ? (before === "dark" ? "light" : "dark") : before);
    assert.equal(await page.evaluate("getComputedStyle(document.activeElement).outlineStyle"), "solid", `${label}: keyboard theme focus`);
  }
  await page.evaluate("document.activeElement.blur(); window.scrollTo({top: 0, behavior: 'instant'})");
}

async function checkAssociationDefinitions(page, label) {
  const definitions = await page.evaluate("[...document.querySelectorAll('.association-definition')].map(detail => ({ id: detail.id, summary: detail.querySelector('summary').textContent, formulae: detail.querySelectorAll('math').length, open: detail.open }))");
  assert.deepEqual(definitions.map(detail => detail.id), ["mi", "mi3", "t-score", "z-score", "g2", "log-ratio"].map(measure => `association-${measure}`));
  assert.ok(definitions.every(detail => detail.formulae > 0 && !detail.open), `${label}: definitions must be typeset and initially disclosed on request`);
  await page.evaluate("document.querySelector('#association-mi summary').focus()");
  for (const expected of [true, false]) {
    // Native summary activation needs Enter's character event, not only keydown/keyup.
    await browser.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: "\r", unmodifiedText: "\r" }, page.sessionId);
    await browser.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 }, page.sessionId);
    assert.equal(await page.evaluate("document.querySelector('#association-mi').open"), expected, `${label}: native disclosure keyboard operation`);
    assert.equal(await page.evaluate("getComputedStyle(document.activeElement).outlineStyle"), "solid", `${label}: disclosure focus`);
  }
  await page.evaluate("document.activeElement.blur(); window.scrollTo({top: 0, behavior: 'instant'})");
}

async function checkSharedTheme() {
  const page = await open("/about.html", 1440, "light", { resetTheme: false });
  try {
    await browser.send("Page.addScriptToEvaluateOnNewDocument", { source: "requestAnimationFrame(() => window.firstFrameTheme = document.documentElement.dataset.theme);" }, page.sessionId);
    await page.evaluate("localStorage.setItem('theme', 'dark')");
    await page.navigate(base + "/about.html");
    await waitFor(page, "document.readyState === 'complete' && window.firstFrameTheme === 'dark'");
    assert.equal(await page.evaluate("document.querySelector('#theme-toggle').getAttribute('aria-pressed')"), "true");
    await page.evaluate("document.querySelector('#theme-toggle').click()");
    assert.equal(await page.evaluate("localStorage.getItem('theme')"), "light");
    await page.navigate(base + "/quick-start.html");
    await waitFor(page, "document.readyState === 'complete' && window.firstFrameTheme === 'light'");
    assert.equal(await page.evaluate("document.querySelector('#theme-toggle').getAttribute('aria-pressed')"), "false");
    await browser.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: "dark" }] }, page.sessionId);
    await browser.send("Page.addScriptToEvaluateOnNewDocument", { source: "for (const method of ['getItem', 'setItem']) Storage.prototype[method] = () => { throw new Error('Storage disabled for acceptance test'); };" }, page.sessionId);
    await page.navigate(base + "/about.html");
    await waitFor(page, "document.readyState === 'complete' && window.firstFrameTheme === 'dark'");
    await page.evaluate("document.querySelector('#theme-toggle').click()");
    assert.equal(await page.evaluate("document.documentElement.dataset.theme"), "light");
    assert.deepEqual(browserFailures(browser, page.sessionId), []);
    cases.push({ label: "shared-theme", persistedAcrossPages: true, prePaintTheme: true, disabledStorage: true, browserErrors: 0 });
  } finally { await page.close(); }
}

async function loadVisibleImages(page) {
  // A jump to the footer skips lazy figures in the middle of a long Methods page.
  const pending = await page.evaluate("[...document.images].flatMap((image, index) => image.getClientRects().length && (!image.complete || !image.naturalWidth) ? [index] : [])");
  for (const index of pending) {
    await page.evaluate(`document.images[${index}].scrollIntoView({ block: 'center', behavior: 'instant' })`);
    await waitFor(page, `document.images[${index}].complete && document.images[${index}].naturalWidth > 0`);
  }
}

async function capture(page, filename, fullPage = false) {
  let options = {};
  if (fullPage) {
    const metrics = await browser.send("Page.getLayoutMetrics", {}, page.sessionId);
    options = { captureBeyondViewport: true, clip: { x: 0, y: 0, width: metrics.cssContentSize.width, height: metrics.cssContentSize.height, scale: 1 } };
  }
  writeFileSync(join(output, filename), await page.screenshot(options));
}

async function checkInlineWalkthrough(page, label, enhanced) {
  const state = await page.evaluate(`(() => {
    const tour = document.querySelector('.walkthrough'), stage = tour.querySelector('.walkthrough-stage');
    return {
      enhanced: tour.classList.contains('is-enhanced'),
      stageVisible: stage.getClientRects().length > 0,
      stageAriaHidden: stage.getAttribute('aria-hidden'),
      stageTabStops: [...stage.querySelectorAll('a[href], button, input, summary, [tabindex]')]
        .filter(element => element.tabIndex >= 0 && element.getClientRects().length).length,
      steps: [...tour.querySelectorAll('[data-walk-step]')].map(step => {
        const heading = step.querySelector('h2'), paragraphs = [...step.querySelectorAll('p')];
        const figure = step.querySelector('figure'), caption = figure?.querySelector('figcaption');
        const media = figure?.querySelector('img, pre');
        const visible = element => element?.getClientRects().length > 0
          && getComputedStyle(element).visibility !== 'hidden';
        return {
          name: step.dataset.walkStep,
          heading: heading?.textContent.trim(),
          caption: caption?.textContent.trim(),
          alt: media?.tagName === 'IMG' ? media.alt : null,
          kind: media?.tagName,
          accessible: !figure?.closest('[hidden], [aria-hidden="true"], [inert]') && visible(caption),
          inline: visible(media) && figure.getBoundingClientRect().top >= Math.max(
            heading?.getBoundingClientRect().bottom ?? Infinity,
            ...paragraphs.filter(paragraph => !figure.contains(paragraph)).map(paragraph => paragraph.getBoundingClientRect().bottom),
          ) - 1,
          captionBelow: visible(media) && caption?.getBoundingClientRect().top >= media.getBoundingClientRect().bottom - 1,
          imageLoading: media?.tagName === 'IMG' ? media.loading : null,
        };
      }),
    };
  })()`);
  assert.equal(state.enhanced, enhanced, `${label}: enhancement state`);
  assert.equal(state.stageVisible, enhanced, `${label}: stage must appear only with JavaScript`);
  assert.equal(state.stageAriaHidden, "true", `${label}: duplicate stage must be hidden from assistive technology`);
  assert.equal(state.stageTabStops, 0, `${label}: duplicate stage must be omitted from tab order`);
  assert.deepEqual(state.steps.map(step => step.name), stages, `${label}: seven ordered walkthrough steps`);
  for (const [index, step] of state.steps.entries()) {
    assert.ok(step.heading, `${label}: ${step.name} needs a heading`);
    assert.match(step.caption ?? "", new RegExp(`^Figure ${index + 1}\\b`), `${label}: numbered ${step.name} caption`);
    assert.ok(step.accessible, `${label}: ${step.name} caption must remain accessible`);
    assert.equal(step.kind, step.name === "record" ? "PRE" : "IMG", `${label}: ${step.name} figure medium`);
    if (step.kind === "IMG") {
      assert.ok(step.alt?.trim(), `${label}: ${step.name} capture needs useful alternative text`);
      if (index > 0) assert.equal(step.imageLoading, "lazy", `${label}: defer later captures`);
    }
    if (!enhanced) {
      assert.ok(step.inline, `${label}: ${step.name} figure must follow the prose it explains`);
      assert.ok(step.captionBelow, `${label}: ${step.name} caption follows the media`);
    }
  }
}

async function checkStageMotion(page, label, reducedMotion) {
  const longest = await page.evaluate(`(() => {
    const stage = document.querySelector('.walkthrough-stage');
    const milliseconds = value => Number.parseFloat(value) * (value.trim().endsWith('ms') ? 1 : 1000);
    const times = [stage, ...stage.querySelectorAll('*')].flatMap(element => {
      const style = getComputedStyle(element);
      return (style.transitionDuration + ',' + style.animationDuration).split(',').map(milliseconds);
    });
    return Math.max(0, ...times, ...stage.getAnimations({ subtree: true }).map(animation =>
      Number(animation.effect.getTiming().duration) || 0));
  })()`);
  assert.ok(longest <= (reducedMotion ? 0 : 300), `${label}: stage motion duration ${longest}ms`);
}

async function scrollToStep(page, step, label) {
  const state = await page.evaluate(`(() => {
    const heading = document.querySelector('[data-walk-step="${step}"] h2');
    const stage = document.querySelector('.walkthrough-stage');
    const bounds = stage.getBoundingClientRect();
    const expected = innerWidth >= 1000
      ? innerHeight * 0.5
      : Math.min(bounds.height + 44, innerHeight - 32);
    const line = Number(stage.dataset.activationY);
    const top = heading.getBoundingClientRect().top;
    window.scrollBy({ top: top - expected + 4, behavior: 'instant' });
    return { expected, line };
  })()`);
  assert.ok(Math.abs(state.expected - state.line) <= 2, `${label}: activation line must follow the stage or viewport centre ${JSON.stringify(state)}`);
  await waitFor(page, `document.querySelector('.walkthrough-stage').dataset.activeStep === '${step}'`);
  const placement = await page.evaluate(`(() => {
    const stage = document.querySelector('.walkthrough-stage');
    return { stageY: stage.getBoundingClientRect().top, stageWidth: stage.getBoundingClientRect().width,
      headingY: document.querySelector('[data-walk-step="${step}"] h2').getBoundingClientRect().top,
      line: Number(stage.dataset.activationY), headerBottom: document.querySelector('.site-header').getBoundingClientRect().bottom };
  })()`);
  assert.ok(Math.abs(placement.headingY - (placement.line - 4)) <= 3,
    `${label}: ${step} heading must be measured just above the switching line ${JSON.stringify(placement)}`);
  assert.ok(placement.stageWidth > 0, `${label}: sticky stage must remain visible`);
  if (label.endsWith("-900") || label.endsWith("-390")) {
    assert.ok(Math.abs(placement.stageY) <= 2, `${label}: mobile stage must pin to viewport top`);
  } else {
    assert.ok(placement.stageY >= placement.headerBottom - 1, `${label}: desktop stage overlaps the header`);
  }
}

async function checkActiveMedia(page, step, theme, label) {
  if (step === "record") {
    const manifest = await page.evaluate("document.querySelector('[data-stage-current] pre')?.textContent.trim()");
    assert.ok(manifest, `${label}: stage must display the real export record excerpt`);
    assert.equal(manifest, await page.evaluate("document.querySelector('[data-walk-step=record] pre')?.textContent.trim()"));
    return;
  }
  await waitFor(page, "(() => { const image = document.querySelector('[data-stage-current] img'); return image?.complete && image.naturalWidth > 0; })()");
  const image = await page.evaluate(`(() => {
    const image = document.querySelector('[data-stage-current] img');
    const inline = document.querySelector('[data-walk-step="${step}"] img');
    const inputs = [inline.src, ...[...inline.closest('picture').querySelectorAll('source')].map(source => new URL(source.srcset, document.baseURI).href)];
    const linked = inline.closest('[data-walk-step]').querySelector('.walk-full-image').href;
    return { url: image.currentSrc, known: inputs.includes(image.currentSrc), linked, size: [image.naturalWidth, image.naturalHeight] };
  })()`);
  assert.equal(image.linked, image.url, `${label}: full-size link must match the displayed capture`);
  assert.equal(image.known, true, `${label}: ${step} must display one of its own real captures`);
  assert.ok(image.url.endsWith(`-${theme}.png`), `${label}: ${step} capture must match the website theme`);
  assert.deepEqual(image.size, [1440, 1000], `${label}: ${step} capture dimensions`);
}

async function checkWalkthrough(page, label, width, theme) {
  const header = ".site-header";
  await waitFor(page, "document.querySelector('.walkthrough')?.classList.contains('is-enhanced')");
  await checkInlineWalkthrough(page, label, true);
  assert.equal(await page.evaluate(`document.querySelector('${header}').classList.contains('is-solid')`), false, `${label}: header over hero`);
  assert.equal(await page.evaluate("getComputedStyle(document.querySelector('.site-header')).backgroundColor"), "rgba(0, 0, 0, 0)", `${label}: transparent hero header`);
  await waitFor(page, "(() => { const image = document.querySelector('.walk-hero-figure img'); return image?.complete && image.naturalWidth === 1440; })()");
  assert.equal(await page.evaluate("document.querySelector('.walk-hero-figure img').loading === 'lazy'"), false, `${label}: hero image must load eagerly`);
  assert.ok((await page.evaluate("document.querySelector('.walk-hero-figure img').currentSrc")).endsWith(`-${theme}.png`), `${label}: hero image theme`);
  await capture(page, `${label}-hero.png`);
  await page.evaluate("window.scrollBy({ top: document.querySelector('.walk-hero').getBoundingClientRect().bottom + 1, behavior: 'instant' })");
  await waitFor(page, "(() => { const header = document.querySelector('.site-header'); return header.classList.contains('is-solid') && getComputedStyle(header).backgroundColor === getComputedStyle(document.body).backgroundColor; })()");
  await page.evaluate("window.scrollTo({ top: 0, behavior: 'instant' })");
  await waitFor(page, "!document.querySelector('.site-header').classList.contains('is-solid')");
  for (const stage of stages) {
    await scrollToStep(page, stage, label);
    await checkActiveMedia(page, stage, theme, label);
    await checkStageMotion(page, label, false);
    if (stage === "cleaning" || stage === "context") {
      await waitFor(page, "document.querySelector('.walkthrough-stage').getAnimations({ subtree: true }).every(animation => animation.playState !== 'running')");
      await capture(page, `${label}-${stage}.png`);
    }
  }
  // Reversing the scroll direction must select the same active step, not retain the last state.
  await scrollToStep(page, "cleaning", label);
  const nextTheme = theme === "dark" ? "light" : "dark";
  await page.evaluate("document.querySelector('#theme-toggle').click()");
  await checkActiveMedia(page, "cleaning", nextTheme, label);
  await page.evaluate("document.querySelector('#theme-toggle').click()");
  await checkActiveMedia(page, "cleaning", theme, label);
  if (width === 390) {
    const dimensions = { width, deviceScaleFactor: 1, mobile: false, screenWidth: width };
    await browser.send("Emulation.setDeviceMetricsOverride", { ...dimensions, height: 640, screenHeight: 640 }, page.sessionId);
    await waitFor(page, "(() => { const stage = document.querySelector('.walkthrough-stage'); return Number(stage.dataset.activationY) === Math.min(innerHeight - 32, stage.offsetHeight + 44); })()");
    await scrollToStep(page, "context", label);
    await checkActiveMedia(page, "context", theme, label);
    await browser.send("Emulation.setDeviceMetricsOverride", { ...dimensions, height: 1000, screenHeight: 1000 }, page.sessionId);
  }
  await checkPage(page, `${label}-walkthrough`);
  return { stages: stages.length, switchedTheme: true, reverseScroll: true, width };
}

try {
  for (const theme of ["light", "dark"]) {
    for (const width of widths) {
      const label = `home-${theme}-${width}`;
      const page = await open("/index.html", width, theme, { reducedMotion: width === 320 });
      try {
        await waitFor(page, "document.querySelector('.walkthrough')?.classList.contains('is-enhanced')");
        const geometry = await checkDesignSystem(page, label);
        assert.equal(await page.evaluate("document.documentElement.dataset.theme"), theme);
        const walkthrough = evidenceWidths.includes(width) ? await checkWalkthrough(page, label, width, theme) : undefined;
        if (width === 320) {
          await page.evaluate("(() => { const stage = document.querySelector('.walkthrough-stage'); const heading = document.querySelector('[data-walk-step=cleaning] h2'); window.scrollBy({ top: heading.getBoundingClientRect().top - Number(stage.dataset.activationY) + 4, behavior: 'instant' }); })()");
          await waitFor(page, "document.querySelector('.walkthrough-stage').dataset.activeStep === 'cleaning'");
          await checkStageMotion(page, label, true);
        }
        await checkKeyboard(page, label);
        cases.push({ label, ...geometry, walkthrough, keyboard: "passed", reducedMotion: width === 320, browserErrors: 0 });
        process.stdout.write(`Passed ${label}\n`);
      } finally { await page.close(); }
    }
  }
  for (const theme of ["light", "dark"]) {
    const label = `home-${theme}-390-fallback`;
    const fallback = await open("/index.html", 390, theme, { javascript: false });
    try {
      await checkInlineWalkthrough(fallback, label, false);
      await loadVisibleImages(fallback);
      assert.equal(await fallback.evaluate("[...document.querySelectorAll('[data-walk-step] img')].every(image => image.complete && image.naturalWidth === 1440)"), true);
      await checkPage(fallback, label);
      cases.push({ label, allStagesAvailable: true, browserErrors: 0 });
    } finally { await fallback.close(); }
  }
  for (const path of readingPages) {
    for (const theme of ["light", "dark"]) {
      for (const width of widths) {
        const page = await open(`/${path}`, width, theme);
        try {
          const label = `${path}-${theme}-${width}`;
          const geometry = await checkDesignSystem(page, label);
          const toc = path === "concepts/overview.html" ? await checkToc(page, label) : undefined;
          if (path === "concepts/overview.html") await checkAssociationDefinitions(page, label);
          await checkKeyboard(page, label);
          await loadVisibleImages(page);
          const figureRequests = await checkFigureRequests(page, label, theme);
          cases.push({ label, ...geometry, toc, figureRequests, browserErrors: browserFailures(browser, page.sessionId).length });
          await checkPage(page, label);
          await page.evaluate("document.activeElement.blur(); window.scrollTo({ top: 0, behavior: 'instant' })");
          if (path === "concepts/overview.html" && (width === 1440 || width === 390)) {
            await capture(page, `${label.replaceAll('/', '-')}-top.png`);
          }
        } finally { await page.close(); }
      }
    }
  }
  for (const path of ["index.html", ...readingPages]) {
    for (const theme of ["light", "dark"]) {
      for (const [mode, zoom, textScale] of [["zoom-200", 2, 1], ["text-200", 1, 2]]) {
        const label = `${path.replaceAll('/', '-')}-${theme}-${mode}`;
        const page = await open(`/${path}`, 1280, theme, { zoom, textScale });
        try {
          const geometry = await checkDesignSystem(page, label, textScale);
          await loadVisibleImages(page);
          await page.evaluate("window.scrollTo({ top: document.body.scrollHeight, behavior: 'instant' })");
          await checkPage(page, label);
          await page.evaluate("window.scrollTo({ top: 0, behavior: 'instant' })");
          cases.push({ label, ...geometry, zoom, textScale, browserErrors: 0 });
        } finally { await page.close(); }
      }
    }
  }
  for (const path of ["index.html", ...readingPages]) {
    for (const theme of ["light", "dark"]) {
      const label = `${path.replaceAll('/', '-')}-${theme}-no-JavaScript`;
      const page = await open(`/${path}`, 320, theme, { javascript: false });
      try {
        await checkPage(page, label);
        assert.equal(await page.evaluate("document.querySelectorAll('main h1').length"), 1);
        if (path === "concepts/overview.html") assert.equal(await page.evaluate("document.querySelectorAll('.docs-toc a').length"), 16);
        await loadVisibleImages(page);
        cases.push({ label, nativeContentAvailable: true, browserErrors: 0 });
      } finally { await page.close(); }
    }
  }
  await checkSharedTheme();
  writeFileSync(join(output, "manifest.json"), JSON.stringify({ cases }, null, 2) + "\n");
  process.stdout.write(`Public-site browser acceptance passed: ${cases.length} cases.\n`);
} finally {
  await browser.close();
  await new Promise(resolve => server.close(resolve));
  if (!process.argv[2]) rmSync(output, { recursive: true, force: true });
}
