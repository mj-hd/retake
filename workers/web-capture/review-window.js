#!/usr/bin/env node
// A dedicated Playwright browser owned by one retake review. The URL arrives
// over stdin so its access token never appears in the process command line.
import { chromium } from 'playwright';
import { createInterface } from 'node:readline';

async function main() {
  const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
  let browser;
  let page;
  let closing = false;
  const close = async () => {
    if (closing) return;
    closing = true;
    lines.close();
    if (browser?.isConnected()) await browser.close().catch(() => {});
  };
  const closeAndExit = code => { void close().finally(() => process.exit(code)); };
  process.once('SIGINT', () => closeAndExit(0));
  process.once('SIGTERM', () => closeAndExit(0));
  process.once('SIGHUP', () => closeAndExit(0));

  try {
    for await (const line of lines) {
      if (!browser) {
        const { url } = JSON.parse(line);
        const parsed = new URL(url);
        if (parsed.protocol !== 'http:' || parsed.hostname !== '127.0.0.1' || !/^\/review\/[0-9a-f-]+$/.test(parsed.pathname)) {
          throw new Error('expected a local retake review URL');
        }
        browser = await chromium.launch({
          headless: process.env.RETAKE_BROWSER_HEADLESS === '1',
          ...(process.env.RETAKE_CHROMIUM_BIN ? { executablePath: process.env.RETAKE_CHROMIUM_BIN } : {}),
        });
        // browser.close() also emits "disconnected". Exiting from that event
        // interrupts Playwright's own shutdown and can strand the Chromium
        // leader after replacing or cancelling a review window.
        browser.on('disconnected', () => { if (!closing) process.exit(0); });
        // Playwright's default page has a fixed 1280×720 viewport. Use the
        // native window size so resizing the review window reflows the UI.
        const context = await browser.newContext({ viewport: null });
        page = await context.newPage();
        // Closing the native window (red button/Cmd+W) closes the browser too,
        // instead of leaving a windowless Chrome app in the macOS Dock.
        page.once('close', () => { if (!closing) closeAndExit(0); });
        await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 15000 });
        process.stdout.write('ready\n');
      } else if (line === 'close') {
        await close();
        return;
      } else if (line === 'focus') {
        // The review owns this browser, so activating its only page is safe.
        // Failures are non-fatal: the revision is still available even when
        // the window manager refuses to move an app to the foreground.
        await page?.bringToFront().catch(() => {});
        await page?.evaluate(() => window.focus()).catch(() => {});
      }
    }
  } finally {
    // stdin closes when the MCP server exits, including abrupt host shutdown.
    await close();
  }
}

main().catch(error => { console.error(error); process.exit(1); });
