/** Browser acceptance for the shared site geometry, reading hierarchy and manual GUI gallery. */
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { extname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { browserFailures, createCapturePage, launchCaptureBrowser, waitFor } from "../apps/desktop/scripts/browser-capture.mjs";

const docs = fileURLToPath(new URL("../docs", import.meta.url));
const output = process.argv[2] ?? "/tmp/corpuswright-public-site-qa";
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
const widths = [1440, 1280, 1024, 900, 880, 840, 820, 800, 768, 740, 721, 720, 375, 320];
const stages = ["prepare", "overview", "concordance"];
const readingPages = ["quick-start.html", "concepts/overview.html", "about.html", "citation.html", "technical-docs.html", "roadmap.html", "404.html"];

function contrastRatio(foreground, background) {
  const luminance = color => {
    const values = color.match(/[\d.]+/g).slice(0, 3).map(Number).map(value => {
      const channel = value / 255;
      return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
    });
    return values[0] * 0.2126 + values[1] * 0.7152 + values[2] * 0.0722;
  };
  const values = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  return (values[0] + 0.05) / (values[1] + 0.05);
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
  await page.evaluate("window.MathJax?.startup?.promise");
  await page.evaluate("document.fonts.ready");
  if (javascript && path === "/concepts/overview.html") {
    assert.equal(await page.evaluate("document.querySelectorAll('.math-display').length"), await page.evaluate("document.querySelectorAll('.math-display mjx-container').length"), "Every display formula must be typeset");
    assert.equal(await page.evaluate("document.querySelectorAll('mjx-merror').length"), 0, "Methods formula parse errors");
    // Mermaid starts after page load; measuring early can truncate a full-page capture.
    await waitFor(page, "document.querySelectorAll('pre.mermaid svg').length === 3");
  }
  return page;
}

async function checkPage(page, label) {
  const failures = browserFailures(browser, page.sessionId);
  assert.deepEqual(failures, [], `${label}: browser errors`);
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

async function checkDesignSystem(page, label, textScale = 1) {
  const geometry = await checkPage(page, label);
  const reading = await page.evaluate(`(() => {
    const h1 = document.querySelector('h1');
    const h2 = document.querySelector('main h2:not(#docs-toc-title)');
    const paragraph = document.querySelector('.home-definition > p:not(.lede):not(.alpha-note), .learn-section > p, .shell.narrow > p');
    const body = getComputedStyle(document.body);
    const style = getComputedStyle(paragraph);
    const canvas = document.createElement('canvas').getContext('2d');
    canvas.font = style.font;
    const footer = document.querySelector('.site-footer .shell');
    const header = document.querySelector('.page-intro');
    const shell = document.querySelector('.site-header .shell');
    return {
      fonts: [...document.querySelectorAll('main h1, main h2, main h3, main p, .docs-toc, .nav-links, figcaption')].map(element => getComputedStyle(element).fontFamily),
      weights: [...document.querySelectorAll('main h1, main h2, main h3')].filter(element => !element.closest('.docs-toc')).map(element => getComputedStyle(element).fontWeight),
      groupWeights: [...document.querySelectorAll('.toc-group h3')].map(element => getComputedStyle(element).fontWeight),
      mediumLoaded: [...document.fonts].some(face => face.family === 'IBM Plex Sans' && face.weight === '500' && face.status === 'loaded'),
      italicLoaded: [...document.fonts].some(face => face.family === 'IBM Plex Sans' && face.weight === '400' && face.style === 'italic' && face.status === 'loaded'),
      fontSynthesis: body.fontSynthesis,
      role: document.body.classList.contains('home') ? 'home' : document.body.classList.contains('methods') ? 'methods' : 'guide-about',
      headingLeading: [h1, h2].filter(Boolean).map(element => parseFloat(getComputedStyle(element).lineHeight) / parseFloat(getComputedStyle(element).fontSize)),
      targets: [...document.querySelectorAll('.brand, .nav-links a, .theme-btn, .btn, .gallery-tabs button')].map(element => ({ height: element.getBoundingClientRect().height, width: element.getBoundingClientRect().width })),
      themeBorder: getComputedStyle(document.querySelector('.theme-btn')).borderTopWidth,
      navigationRows: new Set([...document.querySelectorAll('.nav-links a')].map(element => Math.round(element.getBoundingClientRect().top))).size,
      headerHeight: document.querySelector('.site-header').getBoundingClientRect().height,
      headerControls: [...document.querySelectorAll('.brand, .nav-links a, .theme-btn')].map(element => {
        const rect = element.getBoundingClientRect();
        return { left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom };
      }),
      galleryTabHeights: [...document.querySelectorAll('.gallery-tabs button')].map(element => element.getBoundingClientRect().height),
      guideFigures: [...document.querySelectorAll('.guide-figure')].map(element => ({ width: element.getBoundingClientRect().width, x: element.getBoundingClientRect().x })),
      productFigures: [...document.querySelectorAll('.product-figure')].map(element => ({ width: element.getBoundingClientRect().width, right: element.getBoundingClientRect().right })),
      axis: [document.querySelector('.brand'), h1, h2, paragraph].filter(Boolean).map(element => element.getBoundingClientRect().x),
      footerAxis: footer.getBoundingClientRect().x + parseFloat(getComputedStyle(footer).paddingLeft),
      h1Size: parseFloat(getComputedStyle(h1).fontSize),
      h2Size: h2 ? parseFloat(getComputedStyle(h2).fontSize) : null,
      bodySize: parseFloat(body.fontSize), leading: parseFloat(body.lineHeight) / parseFloat(body.fontSize),
      proseColor: style.color, primaryColor: body.color, backgroundColor: body.backgroundColor,
      captionColor: document.querySelector('figcaption') ? getComputedStyle(document.querySelector('figcaption')).color : null,
      proseCh: paragraph.getBoundingClientRect().width / canvas.measureText('0').width,
      shellWidth: shell.getBoundingClientRect().width,
      headerPadding: header ? [getComputedStyle(header).paddingTop, getComputedStyle(header).paddingBottom].map(parseFloat) : null,
      eyebrows: document.querySelectorAll('.page-intro .eyebrow').length,
      tocPosition: document.querySelector('.docs-toc') ? getComputedStyle(document.querySelector('.docs-toc')).position : null,
    };
  })()`);
  assert.ok(reading.fonts.every(font => font.includes("IBM Plex Sans")), `${label}: structural Sans`);
  assert.ok(reading.weights.every(weight => weight === "600"), `${label}: heading weights`);
  assert.ok(reading.groupWeights.every(weight => weight === "500"), `${label}: TOC group weights`);
  assert.equal(reading.mediumLoaded, true, `${label}: genuine locally hosted Medium face loaded`);
  assert.equal(reading.fontSynthesis, "none", `${label}: font styles must use real faces`);
  if (reading.role === "methods") assert.equal(reading.italicLoaded, true, `${label}: genuine body italic loaded`);
  assert.ok(reading.targets.every(target => target.height >= 44 && target.width >= 44), `${label}: 44px control targets`);
  assert.equal(reading.themeBorder, "0px", `${label}: theme utility must not compete with Download`);
  assert.ok(Math.abs(reading.headingLeading[0] - 1.1) < 0.005, `${label}: compact H1 leading`);
  if (reading.headingLeading[1]) assert.ok(Math.abs(reading.headingLeading[1] - 1.16) < 0.005, `${label}: H2 leading`);
  assert.ok(reading.axis.every(axis => Math.abs(axis - reading.axis[0]) < 1), `${label}: shared left axis ${reading.axis}`);
  assert.ok(Math.abs(reading.footerAxis - reading.axis[0]) < 1, `${label}: footer axis`);
  assert.ok(reading.proseCh <= 72.1, `${label}: prose measure ${reading.proseCh}ch`);
  const expectedLeading = { home: 1.54, methods: 1.64, "guide-about": 1.58 }[reading.role];
  assert.ok(Math.abs(reading.leading - expectedLeading) < 0.005, `${label}: ${reading.role} body leading`);
  assert.equal(reading.proseColor, reading.primaryColor, `${label}: primary prose must use the primary text colour`);
  reading.proseContrast = contrastRatio(reading.proseColor, reading.backgroundColor);
  assert.ok(reading.proseContrast >= 7, `${label}: primary prose contrast ${reading.proseContrast}`);
  assert.equal(reading.eyebrows, 0, `${label}: redundant page eyebrow`);
  if (textScale === 1) {
    assert.ok(reading.h1Size >= 32 && reading.h1Size <= 44, `${label}: title scale`);
    assert.ok(reading.h2Size >= 24 && reading.h2Size <= 28, `${label}: section scale`);
    assert.ok(reading.bodySize >= 16 && reading.bodySize <= 17, `${label}: body scale`);
    assert.ok(!reading.headerPadding || reading.headerPadding.every(padding => padding <= 32), `${label}: compact page header`);
    if (geometry.width >= 720 && geometry.width <= 900) {
      assert.equal(reading.navigationRows, 1, `${label}: intermediate navigation must stay on one row`);
      assert.equal(reading.headerHeight, geometry.width <= 720 ? 109 : 69, `${label}: deliberate header breakpoint`);
      for (let index = 0; index < reading.headerControls.length; index++) {
        for (const other of reading.headerControls.slice(index + 1)) {
          const control = reading.headerControls[index];
          const overlap = control.left < other.right && other.left < control.right
            && control.top < other.bottom && other.top < control.bottom;
          assert.equal(overlap, false, `${label}: header controls must not overlap`);
        }
      }
    }
    if (geometry.width >= 1024) assert.ok(reading.proseCh >= 60, `${label}: prose unnecessarily narrow`);
    if (geometry.width >= 1280) assert.equal(reading.shellWidth, 1180, `${label}: product shell width`);
    if (geometry.width <= 375) {
      assert.equal(reading.navigationRows, 1, `${label}: phone links must compose in one navigation row`);
      assert.equal(reading.headerHeight, 109, `${label}: two-row phone header`);
      assert.ok(reading.galleryTabHeights.every(height => height === 44), `${label}: phone gallery tabs must fit on one line`);
    }
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
  return { ...geometry, ...reading, fonts: undefined, weights: undefined };
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
  const definitions = await page.evaluate("[...document.querySelectorAll('.association-definition')].map(detail => ({ id: detail.id, summary: detail.querySelector('summary').textContent, formulae: detail.querySelectorAll('mjx-container').length, open: detail.open }))");
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

try {
  for (const theme of ["light", "dark"]) {
    for (const width of widths) {
      const label = `home-${theme}-${width}`;
      const page = await open("/index.html", width, theme, { reducedMotion: width === 320 });
      try {
        await waitFor(page, "document.querySelector('.gallery-tabs')?.getAttribute('role') === 'tablist'");
        const geometry = await checkDesignSystem(page, label);
        assert.equal(await page.evaluate("document.documentElement.dataset.theme"), theme);
        for (const stage of stages) {
          await page.evaluate(`document.querySelector('#gallery-tab-${stage}').click()`);
          await waitFor(page, `document.querySelector('#gallery-panel-${stage} img').complete`);
          const state = await page.evaluate(`(() => {
            const tab = document.querySelector('#gallery-tab-${stage}');
            const panel = document.querySelector('#gallery-panel-${stage}');
            const image = panel.querySelector('img');
            const bounds = tab.getBoundingClientRect();
            return { selected: tab.getAttribute('aria-selected'), controlled: tab.getAttribute('aria-controls'),
              labelled: panel.getAttribute('aria-labelledby'), role: panel.getAttribute('role'), hidden: panel.hidden,
              activePanels: document.querySelectorAll('[data-gallery-panel]:not([hidden])').length,
              tabbableTabs: [...document.querySelectorAll('[data-gallery-tab]')].filter(tab => tab.tabIndex === 0).length,
              alt: image.alt, size: [image.naturalWidth, image.naturalHeight],
              bounds: { x: bounds.x, right: bounds.right, width: bounds.width, height: bounds.height },
              captionSize: getComputedStyle(panel.querySelector('figcaption')).fontSize,
              captionVisible: panel.querySelector('figcaption').getBoundingClientRect().height > 0,
              captionBottom: panel.querySelector('figcaption').getBoundingClientRect().bottom, imageTop: image.getBoundingClientRect().top };
          })()`);
          assert.equal(state.selected, "true");
          assert.equal(state.controlled, `gallery-panel-${stage}`);
          assert.equal(state.labelled, `gallery-tab-${stage}`);
          assert.equal(state.role, "tabpanel");
          assert.equal(state.hidden, false);
          assert.equal(state.activePanels, 1);
          assert.equal(state.tabbableTabs, 1);
          assert.ok(state.alt.includes("CorpusWright"));
          assert.deepEqual(state.size, [1440, 1000]);
          assert.ok(state.bounds.height >= 44 && state.bounds.width >= 44);
          assert.ok(state.bounds.x >= 0 && state.bounds.right <= width);
          assert.ok(parseFloat(state.captionSize) >= 16 && parseFloat(state.captionSize) <= 17 && state.captionVisible);
          assert.ok(state.captionBottom <= state.imageTop, "Gallery explanation must precede its screenshot");
          await checkPage(page, `${label}-${stage}`);
          await page.evaluate("window.scrollTo({ top: document.querySelector('[data-product-gallery]').offsetTop - 80, behavior: 'instant' })");
          await capture(page, `${label}-${stage}.png`);
        }
        await page.evaluate("document.querySelector('#gallery-tab-prepare').focus()");
        for (const [key, stage] of [["ArrowRight", "overview"], ["End", "concordance"], ["ArrowRight", "prepare"], ["ArrowLeft", "concordance"], ["Home", "prepare"]]) {
          await browser.send("Input.dispatchKeyEvent", { type: "keyDown", key }, page.sessionId);
          await browser.send("Input.dispatchKeyEvent", { type: "keyUp", key }, page.sessionId);
          const state = await page.evaluate("({ focused: document.activeElement.id, selected: document.activeElement.getAttribute('aria-selected'), outline: getComputedStyle(document.activeElement).outlineStyle })");
          assert.equal(state.focused, `gallery-tab-${stage}`);
          assert.equal(state.selected, "true");
          assert.equal(state.outline, "solid");
        }
        await browser.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Tab" }, page.sessionId);
        await browser.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Tab" }, page.sessionId);
        assert.equal(await page.evaluate("document.activeElement.id"), "gallery-panel-prepare");
        await page.evaluate("document.querySelector('#theme-toggle').click()");
        assert.equal(await page.evaluate("document.documentElement.dataset.theme"), theme === "dark" ? "light" : "dark");
        assert.equal(await page.evaluate("document.querySelector('#gallery-panel-prepare img').getAttribute('src')"), "assets/screenshots/corpuswright-prepare.png");
        await page.evaluate("document.querySelector('#theme-toggle').click(); document.activeElement.blur(); window.scrollTo({ top: 0, behavior: 'instant' })");
        await capture(page, `${label}-full.png`, true);
        cases.push({ label, ...geometry, stages: 3, keyboard: "passed", reducedMotion: width === 320, browserErrors: 0 });
        process.stdout.write(`Passed ${label}\n`);
      } finally { await page.close(); }
    }
  }
  const fallback = await open("/index.html", 375, "light", { javascript: false });
  try {
    assert.equal(await fallback.evaluate("document.querySelector('.gallery-tabs').hidden"), true);
    assert.equal(await fallback.evaluate("document.querySelectorAll('[data-gallery-panel]:not([hidden])').length"), 3);
    await fallback.evaluate("window.scrollTo(0, document.body.scrollHeight)");
    await waitFor(fallback, "[...document.querySelectorAll('.gallery-image img')].every(image => image.complete && image.naturalWidth === 1440)");
    await checkPage(fallback, "no-JavaScript");
    cases.push({ label: "no-JavaScript", allStagesAvailable: true, browserErrors: 0 });
  } finally { await fallback.close(); }
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
          await capture(page, `${label.replaceAll('/', '-')}-top.png`);
          await capture(page, `${label.replaceAll('/', '-')}-full.png`, true);
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
          await capture(page, `${label}.png`);
          if (path === "concepts/overview.html") {
            await page.evaluate("document.querySelector('#how-to-read-this-section').scrollIntoView({ block: 'start', behavior: 'instant' })");
            await capture(page, `${label}-prose.png`);
          }
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
        await capture(page, `${label}.png`);
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
}
