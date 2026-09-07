#!/usr/bin/env node
// Generates the icon set at src-tauri/icons/ from a solid disk with a "CW"
// glyph. Replace later with `npx @tauri-apps/cli icon <source.png>`.
//
// Tray colours map to the runtime states in src-tauri/src/tray.rs:
//   tray-idle  grey    paused / no snapshot yet
//   tray-ok    green   healthy
//   tray-attn  amber   orphan processes present
//   tray-warn  red     launch failure detected / repairing

import { writeFileSync, mkdirSync } from "node:fs";
import { deflateSync } from "node:zlib";
import { Buffer } from "node:buffer";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const iconsDir = path.resolve(__dirname, "..", "src-tauri", "icons");
mkdirSync(iconsDir, { recursive: true });

const BRAND = [217, 119, 87, 255]; // Claude terracotta — app icon
const IDLE = [107, 114, 128, 255]; // gray-500
const OK = [16, 185, 129, 255]; // emerald-500
const ATTN = [245, 158, 11, 255]; // amber-500
const WARN = [239, 68, 68, 255]; // red-500
const FG = [255, 255, 255, 255];

// 5x7 glyphs, 1 = filled.
const GLYPH_C = [
  [0, 1, 1, 1, 1],
  [1, 0, 0, 0, 0],
  [1, 0, 0, 0, 0],
  [1, 0, 0, 0, 0],
  [1, 0, 0, 0, 0],
  [1, 0, 0, 0, 0],
  [0, 1, 1, 1, 1],
];
const GLYPH_W = [
  [1, 0, 0, 0, 1],
  [1, 0, 0, 0, 1],
  [1, 0, 0, 0, 1],
  [1, 0, 1, 0, 1],
  [1, 0, 1, 0, 1],
  [1, 1, 0, 1, 1],
  [1, 0, 0, 0, 1],
];
const TEXT_W = 11; // 5 + 1 gap + 5
const TEXT_H = 7;

function crc32(buf) {
  const table = new Uint32Array(256);
  for (let i = 0; i < 256; i++) {
    let c = i;
    for (let j = 0; j < 8; j++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[i] = c;
  }
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = table[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, "ascii");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0);
  return Buffer.concat([len, typeBuf, data, crc]);
}

function drawText(raw, size, color) {
  const scale = Math.max(1, Math.floor(Math.min((size * 0.68) / TEXT_W, (size * 0.68) / TEXT_H)));
  const x0 = Math.floor((size - TEXT_W * scale) / 2);
  const y0 = Math.floor((size - TEXT_H * scale) / 2);
  const stride = 1 + size * 4;
  const [cr, cg, cb, ca] = color;
  const set = (px, py) => {
    if (px < 0 || px >= size || py < 0 || py >= size) return;
    const idx = py * stride + 1 + px * 4;
    raw[idx] = cr;
    raw[idx + 1] = cg;
    raw[idx + 2] = cb;
    raw[idx + 3] = ca;
  };
  const blit = (glyph, gx, gy) => {
    for (let r = 0; r < 7; r++)
      for (let c = 0; c < 5; c++) {
        if (!glyph[r][c]) continue;
        for (let dy = 0; dy < scale; dy++)
          for (let dx = 0; dx < scale; dx++) set(gx + c * scale + dx, gy + r * scale + dy);
      }
  };
  blit(GLYPH_C, x0, y0);
  blit(GLYPH_W, x0 + 6 * scale, y0);
}

function makePng(size, color, withText = true) {
  const [r, g, b, a] = color;
  const sig = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  const stride = 1 + size * 4;
  const raw = Buffer.alloc(size * stride);
  const cx = size / 2;
  const cy = size / 2;
  const radius = size * 0.48;
  for (let y = 0; y < size; y++) {
    const rowStart = y * stride;
    raw[rowStart] = 0;
    for (let x = 0; x < size; x++) {
      const idx = rowStart + 1 + x * 4;
      const dist = Math.sqrt((x - cx) ** 2 + (y - cy) ** 2);
      if (dist > radius) {
        raw[idx + 3] = 0;
      } else {
        const edge = radius - dist;
        raw[idx] = r;
        raw[idx + 1] = g;
        raw[idx + 2] = b;
        raw[idx + 3] = edge >= 1 ? a : Math.round(a * Math.max(0, edge));
      }
    }
  }
  if (withText) drawText(raw, size, FG);
  return Buffer.concat([sig, chunk("IHDR", ihdr), chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
}

function makeIco(pngs) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(pngs.length, 4);
  const entries = [];
  const blobs = [];
  let offset = 6 + 16 * pngs.length;
  for (const { size, png } of pngs) {
    const e = Buffer.alloc(16);
    e[0] = size === 256 ? 0 : size;
    e[1] = size === 256 ? 0 : size;
    e.writeUInt16LE(1, 4);
    e.writeUInt16LE(32, 6);
    e.writeUInt32LE(png.length, 8);
    e.writeUInt32LE(offset, 12);
    entries.push(e);
    blobs.push(png);
    offset += png.length;
  }
  return Buffer.concat([header, ...entries, ...blobs]);
}

function makeIcns(png) {
  const header = Buffer.from("icns", "ascii");
  const size = Buffer.alloc(4);
  size.writeUInt32BE(8 + 8 + png.length, 0);
  const subType = Buffer.from("ic08", "ascii");
  const subSize = Buffer.alloc(4);
  subSize.writeUInt32BE(8 + png.length, 0);
  return Buffer.concat([header, size, subType, subSize, png]);
}

const sizes = [16, 32, 48, 64, 128, 256];
const pngBySize = Object.fromEntries(sizes.map((s) => [s, makePng(s, BRAND)]));

writeFileSync(path.join(iconsDir, "32x32.png"), pngBySize[32]);
writeFileSync(path.join(iconsDir, "128x128.png"), pngBySize[128]);
writeFileSync(path.join(iconsDir, "128x128@2x.png"), pngBySize[256]);
writeFileSync(path.join(iconsDir, "icon.png"), pngBySize[256]);
writeFileSync(path.join(iconsDir, "tray-idle.png"), makePng(64, IDLE));
writeFileSync(path.join(iconsDir, "tray-ok.png"), makePng(64, OK));
writeFileSync(path.join(iconsDir, "tray-attn.png"), makePng(64, ATTN));
writeFileSync(path.join(iconsDir, "tray-warn.png"), makePng(64, WARN));
writeFileSync(path.join(iconsDir, "icon.ico"), makeIco(sizes.map((s) => ({ size: s, png: pngBySize[s] }))));
writeFileSync(path.join(iconsDir, "icon.icns"), makeIcns(pngBySize[256]));

console.log(`icons written to ${iconsDir}`);
