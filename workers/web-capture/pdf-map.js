#!/usr/bin/env node
// PDF -> stitched page PNG + per-page text map (pdfjs-dist + @napi-rs/canvas).
// Usage: node pdf-map.js <file.pdf> <output-dir>
// stdout: { png_path, width, height, pages: [{ page, y, width, height, text: [...] }] }
// Widths/heights are CSS pixels; the PNG is rendered at 2x.
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { getDocument, Util } from 'pdfjs-dist/legacy/build/pdf.mjs';
import { createCanvas, DOMMatrix, ImageData } from '@napi-rs/canvas';

globalThis.DOMMatrix = DOMMatrix;
globalThis.ImageData = ImageData;
// pdfjs logs font warnings through console.log, which must not corrupt JSON stdout.
console.log = (...args) => console.error(...args);

const SCALE = 2;
const MAX_LOGICAL_HEIGHT = 12000;

const file = process.argv[2];
const outDir = process.argv[3];
if (!file || !outDir) {
  console.error('usage: node pdf-map.js <file.pdf> <output-dir>');
  process.exit(1);
}
await fs.promises.mkdir(outDir, { recursive: true });

const data = new Uint8Array(fs.readFileSync(file));
// Node does not bundle PDF.js's standard font outlines. Without these, text
// extraction still works but Helvetica/Times/Courier glyphs silently disappear
// from the rendered PNG ("Helvetica_path_* isn't resolved yet" warnings).
const standardFontDataUrl = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  'node_modules', 'pdfjs-dist', 'standard_fonts',
) + path.sep;
const doc = await getDocument({ data, isEvalSupported: false, standardFontDataUrl }).promise;
if (doc.numPages < 1 || doc.numPages > 40) {
  throw new Error(`PDF must contain 1–40 pages (found ${doc.numPages})`);
}
const pageCount = doc.numPages;

// First pass: page boxes to fit every page on one 960px-wide column.
const boxes = [];
for (let i = 1; i <= pageCount; i++) {
  const page = await doc.getPage(i);
  const vp = page.getViewport({ scale: 1 });
  boxes.push({ pw: vp.width, ph: vp.height });
}
const baseWidth = 960;
const gap = 24;
const natural = boxes.map(b => ({ width: baseWidth, height: baseWidth * (b.ph / b.pw) }));
let total = natural.reduce((a, b) => a + b.height, 0) + gap * (natural.length + 1);
const shrink = total > MAX_LOGICAL_HEIGHT ? (MAX_LOGICAL_HEIGHT - gap * (natural.length + 1)) / (total - gap * (natural.length + 1)) : 1;
// Floor dimensions so rounding cannot push the stitched image above the cap.
const width = Math.floor(baseWidth * shrink);
const heights = boxes.map(b => Math.floor(width * b.ph / b.pw));
total = heights.reduce((a, b) => a + b, 0) + gap * (heights.length + 1);

const stitch = createCanvas(width * SCALE, Math.round(total * SCALE));
const ctx = stitch.getContext('2d');
ctx.fillStyle = '#e9ecf2';
ctx.fillRect(0, 0, stitch.width, stitch.height);

const pages = [];
let cursor = gap;
for (let i = 1; i <= pageCount; i++) {
  const page = await doc.getPage(i);
  const h = heights[i - 1];
  const viewport = page.getViewport({ scale: (width / boxes[i - 1].pw) * SCALE });
  const canvas = createCanvas(Math.round(viewport.width), Math.round(viewport.height));
  const pctx = canvas.getContext('2d');
  await page.render({ canvasContext: pctx, viewport }).promise;
  ctx.drawImage(canvas, 0, Math.round(cursor * SCALE));

  // Text items in logical page coordinates.
  const text = [];
  try {
    const content = await page.getTextContent();
    const logicalViewport = page.getViewport({ scale: width / boxes[i - 1].pw });
    for (const item of content.items) {
      const str = (item.str || '').trim();
      if (!str) continue;
      const s = width / boxes[i - 1].pw;
      const transformed = Util.transform(logicalViewport.transform, item.transform);
      const x = transformed[4];
      const yBaseline = transformed[5];
      const w = (item.width || str.length * 4) * s;
      const hgt = Math.abs(transformed[3]) || Math.abs(item.height || 12) * s;
      text.push({
        text: str.slice(0, 120),
        x: Math.round(x),
        y: Math.round(cursor + yBaseline - hgt),
        width: Math.round(w),
        height: Math.round(Math.max(hgt, 8)),
      });
    }
  } catch {
    // Text extraction is optional; coordinates fall back to page level.
  }
  pages.push({ page: i, y: Math.round(cursor), width, height: h, text });
  cursor += h + gap;
}

const pngPath = path.join(outDir, 'pages.png');
await fs.promises.writeFile(pngPath, await stitch.encode('png'));
process.stdout.write(
  JSON.stringify({ png_path: pngPath, width, height: Math.round(total), pages }),
);
