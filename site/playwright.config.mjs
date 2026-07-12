import fs from "node:fs";
import { defineConfig } from "@playwright/test";

const localChrome = "/usr/bin/google-chrome";
const executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH || (fs.existsSync(localChrome) ? localChrome : undefined);

export default defineConfig({
  testDir: "tests",
  testMatch: "browser.spec.ts",
  fullyParallel: false,
  forbidOnly: true,
  retries: 0,
  workers: 1,
  reporter: "line",
  use: {
    baseURL: "http://127.0.0.1:4173",
    browserName: "chromium",
    launchOptions: executablePath ? { executablePath } : {},
    viewport: { width: 1440, height: 900 },
    colorScheme: "light"
  },
  webServer: {
    command: "node scripts/serve.mjs",
    port: 4173,
    reuseExistingServer: false,
    timeout: 30_000
  }
});
