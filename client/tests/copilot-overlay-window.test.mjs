import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const componentPath = resolve(__dirname, "../src/views/CopilotOverlayWindow.vue");
const source = readFileSync(componentPath, "utf8");

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

const singleAutoClosePattern = /if \(mode\.value === "single"\) \{[\s\S]*?setTimeout\(\(\) => closeWindow\(\), 2000\);[\s\S]*?\}/;

assert(
  !singleAutoClosePattern.test(source),
  "single-mode auto-close must not call closeWindow(), because closeWindow() sends /cancel and cancel_copilot",
);

const dismissFunctionPattern = /async function dismissWindow\(\) \{[\s\S]*?await invoke\("copilot_close"\);[\s\S]*?\}/;

assert(
  dismissFunctionPattern.test(source),
  "single-mode auto-close should use dismissWindow(), which only closes the overlay",
);

console.log("Copilot overlay single-mode auto-close behavior is safe.");
