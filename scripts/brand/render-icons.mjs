import { chromium } from "playwright";
import fs from "node:fs";
const src = "data:image/png;base64," + fs.readFileSync("assets/brand/logo.png").toString("base64");
const b = await chromium.launch(); const p = await b.newPage();
const out = await p.evaluate(async (src) => {
  const logo = new Image(); logo.src = src; await logo.decode();
  const png = (c) => c.toDataURL("image/png");

  // macOS-style app icon: 824px rounded tile in a 1024 canvas, midnight
  // gradient with a soft periwinkle glow, logo centred.
  function appIcon(S) {
    const c = document.createElement("canvas"); c.width = c.height = S;
    const g = c.getContext("2d"); const k = S / 1024;
    const x = 100 * k, y = 100 * k, w = 824 * k, r = 185 * k;
    const tile = new Path2D(); tile.roundRect(x, y, w, w, r);
    g.save(); g.shadowColor = "rgba(0,0,0,0.35)"; g.shadowBlur = 24 * k; g.shadowOffsetY = 10 * k;
    const bg = g.createLinearGradient(x, y, x + w, y + w);
    bg.addColorStop(0, "#1f2766"); bg.addColorStop(0.55, "#131b4a"); bg.addColorStop(1, "#0a0f2c");
    g.fillStyle = bg; g.fill(tile); g.restore();
    g.save(); g.clip(tile);
    const glow = g.createRadialGradient(x + w * 0.55, y + w * 0.5, 0, x + w * 0.55, y + w * 0.5, w * 0.55);
    glow.addColorStop(0, "rgba(107,123,229,0.38)"); glow.addColorStop(1, "rgba(107,123,229,0)");
    g.fillStyle = glow; g.fillRect(x, y, w, w);
    // Subtle top sheen.
    const sheen = g.createLinearGradient(0, y, 0, y + w * 0.5);
    sheen.addColorStop(0, "rgba(255,255,255,0.08)"); sheen.addColorStop(1, "rgba(255,255,255,0)");
    g.fillStyle = sheen; g.fillRect(x, y, w, w * 0.5);
    const L = w * 0.74; g.imageSmoothingQuality = "high";
    g.drawImage(logo, x + (w - L) / 2, y + (w - L) / 2 + w * 0.01, L, L);
    g.restore();
    g.strokeStyle = "rgba(255,255,255,0.10)"; g.lineWidth = 2 * k; g.stroke(tile);
    return png(c);
  }

  // Menu-bar template image: the logo's silhouette (holes kept), black on
  // transparent. `dot` adds a recording badge.
  function tray(S, dot) {
    const big = document.createElement("canvas"); big.width = big.height = S * 8;
    const g = big.getContext("2d"); const pad = S * 8 * 0.04;
    g.drawImage(logo, pad, pad, S * 8 - 2 * pad, S * 8 - 2 * pad);
    const im = g.getImageData(0, 0, big.width, big.height); const d = im.data;
    for (let i = 0; i < d.length; i += 4) { const a = d[i+3] > 110 ? 255 : 0; d[i] = d[i+1] = d[i+2] = 0; d[i+3] = a; }
    g.putImageData(im, 0, 0);
    if (dot) {
      const R = S * 8 * 0.2, cx = S * 8 - R - pad * 0.2, cy = S * 8 - R - pad * 0.2;
      g.globalCompositeOperation = "destination-out"; g.beginPath(); g.arc(cx, cy, R * 1.35, 0, Math.PI * 2); g.fill();
      g.globalCompositeOperation = "source-over"; g.fillStyle = "#000"; g.beginPath(); g.arc(cx, cy, R, 0, Math.PI * 2); g.fill();
    }
    const c = document.createElement("canvas"); c.width = c.height = S;
    const s = c.getContext("2d"); s.imageSmoothingQuality = "high"; s.drawImage(big, 0, 0, S, S);
    return png(c);
  }
  // The bare logo, for the UI (hero background).
  const small = document.createElement("canvas"); small.width = small.height = 360;
  const sg = small.getContext("2d"); sg.imageSmoothingQuality = "high"; sg.drawImage(logo, 0, 0, 360, 360);
  return { icon: appIcon(1024), icon256: appIcon(256), tray: tray(36, false), trayRec: tray(36, true), logo: png(small) };
}, src);
const files = { icon: "assets/brand/icon-1024.png", icon256: "public/app-icon.png", tray: "src-tauri/icons/tray.png", trayRec: "src-tauri/icons/tray-recording.png", logo: "public/brand-logo.png" };
for (const [k, v] of Object.entries(out)) fs.writeFileSync(files[k], Buffer.from(v.split(",")[1], "base64"));
await b.close();
