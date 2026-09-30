// Renders every icon from the logo (assets/brand/logo-original.webp, a glowing
// "E" on a navy rounded tile):
//   assets/brand/icon-1024.png   macOS app icon (feed to `tauri icon`)
//   public/app-icon.png          the icon for the UI
//   src-tauri/icons/tray*.png    menu-bar template images (a vector E)
// Needs Playwright (`npm i -g playwright`); run from the repository root.
import { chromium } from "playwright";
import fs from "node:fs";

const src = "data:image/webp;base64," + fs.readFileSync("assets/brand/logo-original.webp").toString("base64");
const b = await chromium.launch();
const p = await b.newPage();
const out = await p.evaluate(async (src) => {
  const img = new Image();
  img.src = src;
  await img.decode();
  const W = img.width, H = img.height;
  const c = document.createElement("canvas");
  c.width = W; c.height = H;
  const g = c.getContext("2d");
  g.drawImage(img, 0, 0);
  const d = g.getImageData(0, 0, W, H).data;
  const lum = (x, y) => { const i = (y * W + x) * 4; return Math.max(d[i], d[i + 1], d[i + 2]) * d[i + 3] / 255; };
  const median = (a) => a.sort((x, y) => x - y)[a.length >> 1];

  // The tile's glowing rim: the first bright pixel scanning in from each side.
  const first = (fn) => { for (let t = 0; t < W / 4; t++) if (fn(t) > 170) return t; return 0; };
  const lines = [0.4, 0.45, 0.5, 0.55, 0.6];
  const left = median(lines.map((f) => first((t) => lum(t, Math.round(H * f)))));
  const right = W - 1 - median(lines.map((f) => first((t) => lum(W - 1 - t, Math.round(H * f)))));
  const top = median(lines.map((f) => first((t) => lum(Math.round(W * f), t))));
  const bottom = H - 1 - median(lines.map((f) => first((t) => lum(Math.round(W * f), H - 1 - t))));
  const tile = { x: left, y: top, w: right - left, h: bottom - top };

  // macOS app icon: the artwork cropped just inside its rim, in the standard
  // 824px rounded tile of a 1024 canvas, with a fresh blue-to-orange rim.
  function appIcon(S) {
    const k = S / 1024;
    const cv = document.createElement("canvas"); cv.width = cv.height = S;
    const x = 100 * k, y = 100 * k, w = 824 * k, r = 190 * k;
    const ctx = cv.getContext("2d");
    const shape = new Path2D(); shape.roundRect(x, y, w, w, r);
    ctx.save();
    ctx.shadowColor = "rgba(0,0,0,0.45)"; ctx.shadowBlur = 28 * k; ctx.shadowOffsetY = 12 * k;
    ctx.fillStyle = "#070b26"; ctx.fill(shape);
    ctx.restore();
    ctx.save(); ctx.clip(shape);
    const inset = 0.022; // of the tile: just past the original rim
    const sx = tile.x + tile.w * inset, sy = tile.y + tile.h * inset;
    const sw = tile.w * (1 - 2 * inset), sh = tile.h * (1 - 2 * inset);
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(img, sx, sy, sw, sh, x, y, w, w);
    // Glass sheen across the top.
    const sheen = ctx.createLinearGradient(0, y, 0, y + w * 0.45);
    sheen.addColorStop(0, "rgba(255,255,255,0.10)"); sheen.addColorStop(1, "rgba(255,255,255,0)");
    ctx.fillStyle = sheen; ctx.fillRect(x, y, w, w * 0.45);
    ctx.restore();
    // Rim: warm at the top left, electric blue to violet at the bottom right.
    const rim = ctx.createLinearGradient(x, y, x + w, y + w);
    rim.addColorStop(0, "rgba(255,170,100,0.95)");
    rim.addColorStop(0.45, "rgba(120,150,255,0.8)");
    rim.addColorStop(1, "rgba(110,90,255,0.95)");
    ctx.save();
    ctx.shadowColor = "rgba(90,120,255,0.8)"; ctx.shadowBlur = 10 * k;
    ctx.strokeStyle = rim; ctx.lineWidth = 3.5 * k;
    const inner = new Path2D(); inner.roundRect(x + 2 * k, y + 2 * k, w - 4 * k, w - 4 * k, r - 2 * k);
    ctx.stroke(inner);
    ctx.restore();
    return cv.toDataURL("image/png");
  }

  // Menu-bar template image: the E drawn as two rounded ribbons (the outer
  // curve and the middle arm), black on transparent, like the logo's shape.
  function tray(S, dot) {
    const cv = document.createElement("canvas"); cv.width = cv.height = S;
    const ctx = cv.getContext("2d");
    ctx.scale(S / 36, S / 36);
    ctx.strokeStyle = "#000"; ctx.lineCap = "round"; ctx.lineJoin = "round";
    ctx.lineWidth = 5;
    ctx.stroke(new Path2D("M27.5 9.8 C 22 4.6, 8.6 5.4, 7.4 17.2 C 6.4 28.4, 19.6 31.8, 27.6 26.4"));
    ctx.lineWidth = 4.6;
    ctx.stroke(new Path2D("M14.6 18.6 C 18 15.6, 22.6 15.6, 25.8 18.2"));
    if (dot) {
      ctx.globalCompositeOperation = "destination-out";
      ctx.beginPath(); ctx.arc(29, 29, 7.4, 0, Math.PI * 2); ctx.fill();
      ctx.globalCompositeOperation = "source-over";
      ctx.fillStyle = "#000"; ctx.beginPath(); ctx.arc(29, 29, 5, 0, Math.PI * 2); ctx.fill();
    }
    return cv.toDataURL("image/png");
  }

  return { tile, icon: appIcon(1024), icon256: appIcon(256), tray: tray(36, false), trayRec: tray(36, true) };
}, src);
console.log("tile", out.tile);
const files = {
  icon: "assets/brand/icon-1024.png",
  icon256: "public/app-icon.png",
  tray: "src-tauri/icons/tray.png",
  trayRec: "src-tauri/icons/tray-recording.png",
};
for (const [k, v] of Object.entries(files)) fs.writeFileSync(v, Buffer.from(out[k].split(",")[1], "base64"));
await b.close();
