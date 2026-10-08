#!/usr/bin/env node
import { chromium } from 'playwright';
import fs from 'fs/promises';
import path from 'path';
import { readFileSync } from 'fs';
import { fileURLToPath, pathToFileURL } from 'url';
import MarkdownIt from 'markdown-it';

const mermaidScript = fileURLToPath(new URL('./node_modules/mermaid/dist/mermaid.min.js', import.meta.url));

function escapeHtml(value) {
  return value.replace(/[&<>"']/g, ch => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[ch]);
}

export function renderMarkdown(source, filename, directory) {
  const md = new MarkdownIt({ html: false, linkify: true, typographer: true });
  const defaultFence = md.renderer.rules.fence.bind(md.renderer.rules);
  md.renderer.rules.fence = (tokens, index, options, env, renderer) => {
    const token = tokens[index];
    if (token.info.trim().split(/\s+/)[0] !== 'mermaid') {
      return defaultFence(tokens, index, options, env, renderer);
    }
    const sourceLine = token.map ? token.map[0] + 1 : 1;
    return `<div class="mermaid" data-source-line="${sourceLine}">${escapeHtml(token.content)}</div>`;
  };
  md.core.ruler.push('source_line', state => {
    for (const token of state.tokens) {
      if (token.block && token.map && (token.nesting === 1 || token.type === 'fence' || token.type === 'code_block')) token.attrSet('data-source-line', String(token.map[0] + 1));
    }
  });
  const title = escapeHtml(filename);
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><base href="${pathToFileURL(directory + path.sep).href}"><title>${title}</title>
  <style>
    :root{font-family:system-ui,-apple-system,BlinkMacSystemFont,"Hiragino Kaku Gothic ProN",sans-serif;color:#252935;background:#f1f3f7;font-size:16px;line-height:1.75}
    *{box-sizing:border-box}body{margin:0;padding:48px 32px 80px}.document{max-width:880px;margin:0 auto;padding:48px 64px 72px;background:white;border:1px solid #e2e6ed;border-radius:12px;box-shadow:0 12px 40px #20263810}
    .document-label{font-size:11px;letter-spacing:.14em;font-weight:700;color:#657589;text-transform:uppercase;border-bottom:1px solid #e8eaf0;padding-bottom:14px;margin-bottom:32px}
    h1,h2,h3,h4{font-weight:750;line-height:1.35;letter-spacing:-.03em;color:#1a2232}h1{font-size:2.25em;margin:0 0 28px}h2{font-size:1.55em;border-bottom:1px solid #e5e8ef;padding-bottom:.3em;margin:2em 0 .7em}h3{font-size:1.2em;margin:1.6em 0 .6em}p,ul,ol,blockquote{margin:0 0 1.15em}li{padding-left:.12em}li+li{margin-top:.3em}a{color:#3858a8}strong{color:#202b3e}blockquote{padding:5px 18px;margin-left:0;border-left:4px solid #b6c4e5;color:#56617a;background:#f5f7fc}
    pre{overflow:auto;padding:20px 24px;border-radius:9px;color:#edf0f7;background:#222936;line-height:1.55;font-size:.86em}code{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;background:#f0f2f7;border-radius:4px;padding:.13em .35em;font-size:.88em}pre code{color:inherit;background:none;padding:0;font-size:inherit}hr{border:0;border-top:1px solid #e5e8ef;margin:2em 0}.mermaid{margin:1.5em 0;padding:20px;border:1px solid #e2e6ed;border-radius:9px;background:#fafbfc;text-align:center}.mermaid svg{max-width:100%;height:auto}
    table{width:100%;border-collapse:collapse;margin:1em 0;display:block;overflow-x:auto}th,td{border:1px solid #dfe3ea;padding:9px 12px;text-align:left}th{background:#f5f7fa}img{max-width:100%;height:auto}@media(max-width:680px){body{padding:16px}.document{padding:28px 24px}}
  </style></head><body><article class="document"><div class="document-label">Markdown / ${title}</div>${md.render(source)}</article></body></html>`;
}

async function main() {
  const reqPath = process.argv[2];
  if (!reqPath) {
    console.error('usage: node index.js <request.json>');
    process.exit(1);
  }
  const req = JSON.parse(readFileSync(reqPath, 'utf8'));
  const { version, kind, url, viewport, output_dir } = req;
  if (version !== 1) {
    console.error('bad version');
    process.exit(1);
  }
  const outDir = output_dir;
  await fs.mkdir(outDir, { recursive: true });

  const browser = await chromium.launch({
    headless: true,
    ...(process.env.RETAKE_CHROMIUM_BIN ? { executablePath: process.env.RETAKE_CHROMIUM_BIN } : {}),
  });
  const context = await browser.newContext({
    viewport: { width: viewport.width, height: viewport.height },
    // PNG is 2x; viewport and DOM rectangles remain CSS pixels.
    deviceScaleFactor: 2,
  });
  const page = await context.newPage();

  try {
    if (kind === 'markdown') {
      const docPath = path.resolve(req.path);
      const stat = await fs.stat(docPath);
      if (!stat.isFile() || stat.size > 512 * 1024) throw new Error('Markdown must be a regular file of at most 512 KiB');
      const html = renderMarkdown(await fs.readFile(docPath, 'utf8'), path.basename(docPath), path.dirname(docPath));
      await page.setContent(html, { waitUntil: 'domcontentloaded', timeout: 30000 });
      if (await page.locator('.mermaid').count()) {
        await page.addScriptTag({ path: mermaidScript });
        await page.evaluate(async () => {
          window.mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme: 'neutral' });
          await window.mermaid.run({ querySelector: '.mermaid' });
        });
      }
    } else if (kind === 'html') {
      const fileUrl = url.startsWith('file://') ? url : `file://${path.resolve(req.path || url)}`;
      await page.goto(fileUrl, { waitUntil: 'domcontentloaded', timeout: 30000 });
    } else {
      await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30000 });
    }
    await page.waitForTimeout(300); // allow some render

    // collect visible nodes with rect, xpath-ish, text
    const nodes = await page.evaluate(() => {
      function getXPath(el) {
        if (el.id) return `//*[@id="${el.id}"]`;
        const parts = [];
        while (el && el.nodeType === Node.ELEMENT_NODE) {
          let ix = 0;
          let sibling = el.previousSibling;
          while (sibling) {
            if (sibling.nodeType === Node.ELEMENT_NODE && sibling.tagName === el.tagName) ix++;
            sibling = sibling.previousSibling;
          }
          parts.unshift(el.tagName.toLowerCase() + (ix ? `[${ix+1}]` : ''));
          el = el.parentNode;
        }
        return '/' + parts.join('/');
      }
      const out = [];
      const els = document.querySelectorAll('*');
      const maxNodes = document.querySelector('.document') ? 3000 : 500;
      for (const el of els) {
        const rect = el.getBoundingClientRect();
        if (rect.width < 1 || rect.height < 1) continue;
        const style = window.getComputedStyle(el);
        if (style.visibility === 'hidden' || style.display === 'none' || parseFloat(style.opacity) === 0) continue;
        const text = (el.innerText || el.textContent || '').trim().slice(0, 200);
        out.push({
          xpath: getXPath(el),
          tag: el.tagName.toLowerCase(),
          text,
          rect: { x: Math.round(rect.left), y: Math.round(rect.top), width: Math.round(rect.width), height: Math.round(rect.height) },
          depth: el.closest ? (el.closest('body') ? 1 : 0) : 0, // simple
          attributes: Array.from(el.attributes).reduce((a, at) => { a[at.name] = at.value; return a; }, {})
        });
        if (out.length >= maxNodes) break;
      }
      return out;
    });

    const png_path = path.join(outDir, 'capture.png');
    // Local documents are finite artifacts, so capture their complete scrollable
    // area. A viewport-only HTML screenshot silently discarded content outside
    // 1280x800 and left the review surface with nothing to scroll to. Remote web
    // pages remain viewport captures because they can contain unbounded feeds.
    const fullPage = kind === 'markdown' || kind === 'html';
    const documentSize = fullPage ? await page.evaluate(() => ({
      width: Math.max(document.documentElement.scrollWidth, document.body?.scrollWidth || 0, window.innerWidth),
      height: Math.max(document.documentElement.scrollHeight, document.body?.scrollHeight || 0, window.innerHeight),
    })) : viewport;
    if (documentSize.width > 12000 || documentSize.height > 12000) {
      throw new Error('Local document too large to capture (max 12000 CSS pixels per side)');
    }
    await page.screenshot({ path: png_path, fullPage });

    const resp = {
      version: 1,
      final_url: page.url(),
      title: await page.title(),
      png_path,
      width: documentSize.width,
      height: documentSize.height,
      nodes,
    };
    await fs.writeFile(path.join(outDir, 'response.json'), JSON.stringify(resp));
    console.error('capture ok', png_path);
  } catch (e) {
    console.error('capture err', e);
    // Set the eventual exit status without skipping finally. Calling
    // process.exit here can strand Playwright's detached Chromium on macOS.
    process.exitCode = 2;
  } finally {
    await browser.close();
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(e => { console.error(e); process.exit(1); });
}
