/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_BROWSER_WORKFLOW?: string;
}

declare module "*.txt?raw" {
  const source: string;
  export default source;
}
