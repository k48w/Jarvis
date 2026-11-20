import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { chromium } from "playwright-core";
import { verifyAssistant } from "./scene.mjs";

const directory = await mkdtemp(path.join(tmpdir(), "jarvis-webview-test-"));
const executable = path.resolve(
  process.argv[2] ?? "src-tauri/target/release/JARVIS.exe",
);
const app = spawn(executable, [], {
  windowsHide: true,
  env: {
    ...process.env,
    JARVIS_DATA_DIR: path.join(directory, "data"),
    WEBVIEW2_USER_DATA_FOLDER: directory,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:
      "--remote-debugging-address=127.0.0.1 --remote-debugging-port=0",
  },
});

let browser;
try {
  await new Promise((resolve, reject) => {
    app.once("spawn", resolve);
    app.once("error", reject);
  });
  let port;
  for (let attempt = 0; attempt < 100 && !port; attempt++) {
    assert.equal(app.exitCode, null, "desktop process exited early");
    const files = await readdir(directory, { recursive: true });
    const record = files.find(
      (file) => path.basename(file) === "DevToolsActivePort",
    );
    if (record)
      port = Number.parseInt(
        await readFile(path.join(directory, record), "utf8"),
        10,
      );
    if (!port) await delay(200);
  }
  assert.ok(port, "WebView2 inspection endpoint did not start");
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  const context = browser.contexts()[0];
  let page = context.pages().find((p) => !p.url().includes("window=mascot"));
  if (!page) {
    page = await context.waitForEvent("page", (p) => !p.url().includes("window=mascot"));
  }
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.waitForURL((url) => url.hostname === "tauri.localhost");
  await page.locator('main[data-state="idle"] canvas').first().waitFor();
  await page.evaluate(() => document.fonts.ready.then(() => true));
  // The canvas can be visible before WebGL has drawn the first plasma frame.
  const idlePath = "docs/assistant-idle-desktop.png";
  await page.waitForFunction(() => {
    const canvas = document.querySelector('main[data-state="idle"] canvas');
    return canvas && canvas.width > 0 && canvas.height > 0;
  });
  const canvas = page.locator("main canvas").first();
  const deadline = Date.now() + 30000;
  while (true) {
    const bounds = await canvas.boundingBox();
    assert.ok(bounds, "idle canvas disappeared");
    const png = await page.screenshot({ scale: "css" });
    const cyanVisible = await page.evaluate(
      async ({ encoded, bounds }) => {
        const bytes = Uint8Array.from(atob(encoded), (c) => c.charCodeAt(0));
        const bitmap = await createImageBitmap(
          new Blob([bytes], { type: "image/png" }),
        );
        const sample = new OffscreenCanvas(64, 64).getContext("2d");
        sample.drawImage(
          bitmap,
          bounds.x,
          bounds.y,
          bounds.width,
          bounds.height,
          0,
          0,
          64,
          64,
        );
        bitmap.close();
        const pixels = sample.getImageData(0, 0, 64, 64).data;
        let cyan = 0;
        for (let i = 0; i < pixels.length; i += 4) {
          const [red, green, blue] = pixels.subarray(i, i + 3);
          if (green > 90 && green > red * 1.25 && blue > red * 1.25) cyan++;
        }
        return cyan >= 8;
      },
      { encoded: png.toString("base64"), bounds },
    );
    if (cyanVisible) {
      await writeFile(idlePath, png);
      break;
    }
    assert.ok(
      Date.now() < deadline,
      "default idle canvas did not render a cyan frame",
    );
    await delay(200);
  }
  await verifyAssistant(page, "docs/assistant-desktop.png", {
    isDesktop: true,
  });
  const mascotPage = context
    .pages()
    .find((p) => p.url().includes("window=mascot"));
  assert.ok(mascotPage, "floating desktop mascot window did not spawn");
  await mascotPage.locator(".mascot-pill").waitFor({ timeout: 10000 });
  assert.deepEqual(errors, []);
  console.log("Verified packaged assistant and desktop mascot in WebView2 at", page.url());
} finally {
  await browser?.close();
  app.kill();
}
