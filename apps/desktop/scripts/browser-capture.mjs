// The same headless Chrome/CDP capture mechanism as capture-overview-curation.mjs.
import { mkdtempSync } from "node:fs";
import { rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";

export const delay = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));

export async function launchCaptureBrowser() {
  const profile = mkdtempSync(join(tmpdir(), "corpuswright-public-browser-"));
  const chrome = spawn(process.env.CHROME_BIN ?? "google-chrome", [
    "--headless=new", "--no-sandbox", "--disable-gpu", "--hide-scrollbars",
    "--remote-debugging-port=0", `--user-data-dir=${profile}`, "about:blank",
  ], { stdio: ["ignore", "ignore", "pipe"] });
  const stopped = new Promise(resolve => chrome.once("exit", resolve));
  async function stop() {
    if (chrome.exitCode === null && chrome.signalCode === null) {
      chrome.kill("SIGTERM");
      const deadline = setTimeout(() => chrome.kill("SIGKILL"), 5_000);
      await stopped;
      clearTimeout(deadline);
    }
    await rm(profile, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  }
  let stderr = "";
  try {
    const url = await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error(`Chrome did not start: ${stderr}`)), 10_000);
      chrome.stderr.on("data", chunk => {
        stderr += String(chunk);
        const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
        if (match) { clearTimeout(timeout); resolve(match[1]); }
      });
      chrome.once("exit", code => { clearTimeout(timeout); reject(new Error(`Chrome exited (${code}): ${stderr}`)); });
    });
    const socket = new WebSocket(url);
    const pending = new Map();
    const events = [];
    let nextId = 0;
    socket.addEventListener("message", event => {
      const message = JSON.parse(event.data);
      if (!message.id) { events.push(message); return; }
      const callback = pending.get(message.id);
      if (!callback) return;
      pending.delete(message.id);
      clearTimeout(callback.timeout);
      if (message.error) callback.reject(new Error(message.error.message));
      else callback.resolve(message.result ?? {});
    });
    await new Promise((resolve, reject) => {
      socket.addEventListener("open", resolve, { once: true });
      socket.addEventListener("error", reject, { once: true });
    });
    const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
      const id = ++nextId;
      const timeout = setTimeout(() => {
        pending.delete(id);
        reject(new Error(`CDP timed out: ${method}`));
      }, 20_000);
      pending.set(id, { resolve, reject, timeout });
      socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
    });
    return {
      send, events,
      async close() {
        try { await send("Browser.close"); } finally {
          socket.close();
          await stop();
        }
      },
    };
  } catch (error) {
    await stop();
    throw error;
  }
}

export async function createCapturePage(browser, width, height) {
  const { targetId } = await browser.send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await browser.send("Target.attachToTarget", { targetId, flatten: true });
  await browser.send("Page.enable", {}, sessionId);
  await browser.send("Runtime.enable", {}, sessionId);
  await browser.send("Network.enable", {}, sessionId);
  await browser.send("Emulation.setDeviceMetricsOverride", {
    width, height, deviceScaleFactor: 1, mobile: false, screenWidth: width, screenHeight: height,
  }, sessionId);
  return {
    sessionId,
    evaluate: async expression => {
      const result = await browser.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true }, sessionId);
      if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
      return result.result.value;
    },
    navigate: url => browser.send("Page.navigate", { url }, sessionId),
    screenshot: async (options = {}) => {
      await browser.send("Page.bringToFront", {}, sessionId);
      const result = await browser.send("Page.captureScreenshot", { format: "png", fromSurface: true, ...options }, sessionId);
      return Buffer.from(result.data, "base64");
    },
    close: () => browser.send("Target.closeTarget", { targetId }),
  };
}

export async function waitFor(page, expression) {
  for (let attempt = 0; attempt < 300; attempt += 1) {
    if (await page.evaluate(expression)) return;
    await delay(100);
  }
  throw new Error(`Timed out: ${expression}\n${await page.evaluate("document.body.innerText.slice(0, 3000)")}`);
}

export function browserFailures(browser, sessionId) {
  return browser.events.filter(event => event.sessionId === sessionId && (
    event.method === "Runtime.exceptionThrown"
    || (event.method === "Runtime.consoleAPICalled" && event.params.type === "error")
    || (event.method === "Network.responseReceived" && event.params.response.status >= 400)
    || event.method === "Network.loadingFailed"
  ));
}
