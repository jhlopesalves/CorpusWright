export const config: WebdriverIO.Config = {
  runner: "local",
  specs: ["./tests/browser/**/*.e2e.ts"],
  maxInstances: 1,
  logLevel: "warn",
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: { timeout: 60_000 },
  services: [["@wdio/tauri-service", {
    mode: "browser",
    devServerUrl: "http://127.0.0.1:4173/?browser-workflow=1",
  }]],
  capabilities: [{
    browserName: "tauri",
    "goog:chromeOptions": {
      args: ["--headless=new", "--no-sandbox", "--disable-dev-shm-usage"],
    },
  }],
};
