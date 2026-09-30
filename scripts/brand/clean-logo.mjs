import { chromium } from "playwright";
import fs from "node:fs";
const b = await chromium.launch(); const p = await b.newPage();
const src = "data:image/webp;base64," + fs.readFileSync("assets/brand/logo-original.webp").toString("base64");
const out = await p.evaluate(async (src) => {
  const img = new Image(); img.src = src; await img.decode();
  const W = img.width, H = img.height;
  const c = document.createElement("canvas"); c.width = W; c.height = H;
  const ctx = c.getContext("2d"); ctx.drawImage(img, 0, 0);
  const im = ctx.getImageData(0, 0, W, H); const d = im.data;
  const idx = (x, y) => (y * W + x) * 4;
  // "Halo" = very dark, near-neutral pixels (the cut-out fringe), as opposed to
  // the ribbon's saturated indigo shading.
  const isHalo = (i) => {
    const r = d[i], g = d[i+1], bl = d[i+2], a = d[i+3];
    if (a < 24) return true;
    const max = Math.max(r, g, bl), min = Math.min(r, g, bl);
    return max < 58 && (max - min) < 40;
  };
  // Flood fill from every transparent pixel through halo pixels.
  const seen = new Uint8Array(W * H); const stack = [];
  for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) if (d[idx(x, y) + 3] < 24) { seen[y*W+x] = 1; stack.push(x, y); }
  let cleared = 0;
  while (stack.length) {
    const y = stack.pop(), x = stack.pop();
    for (const [dx, dy] of [[1,0],[-1,0],[0,1],[0,-1]]) {
      const nx = x + dx, ny = y + dy;
      if (nx < 0 || ny < 0 || nx >= W || ny >= H || seen[ny*W+nx]) continue;
      const i = idx(nx, ny);
      if (isHalo(i)) { seen[ny*W+nx] = 1; if (d[i+3] >= 24) { d[i+3] = 0; cleared++; } stack.push(nx, ny); }
    }
  }
  // Soften the edge: alpha = min(alpha, fraction of opaque neighbours in a 5x5 box).
  const alpha = new Uint8Array(W * H); for (let k = 0; k < W*H; k++) alpha[k] = d[k*4+3];
  for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) {
    const k = y*W+x; if (!alpha[k]) continue;
    let sum = 0, cnt = 0;
    for (let yy = -2; yy <= 2; yy++) for (let xx = -2; xx <= 2; xx++) {
      const X = x+xx, Y = y+yy; if (X<0||Y<0||X>=W||Y>=H) { cnt++; continue; }
      sum += alpha[Y*W+X] > 128 ? 1 : 0; cnt++;
    }
    d[k*4+3] = Math.min(alpha[k], Math.round(255 * Math.min(1, (sum / cnt) * 1.6)));
  }
  ctx.putImageData(im, 0, 0);
  // Trim to content bounds and pad to a square.
  let minX = W, minY = H, maxX = 0, maxY = 0;
  for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) if (d[idx(x,y)+3] > 16) { minX = Math.min(minX,x); maxX = Math.max(maxX,x); minY = Math.min(minY,y); maxY = Math.max(maxY,y); }
  const bw = maxX - minX + 1, bh = maxY - minY + 1, S = Math.max(bw, bh);
  const sq = document.createElement("canvas"); sq.width = sq.height = S;
  sq.getContext("2d").drawImage(c, minX, minY, bw, bh, (S - bw) / 2, (S - bh) / 2, bw, bh);
  return { cleared, box: [minX, minY, bw, bh], png: sq.toDataURL("image/png") };
}, src);
fs.writeFileSync("assets/brand/logo.png", Buffer.from(out.png.split(",")[1], "base64"));
console.log("cleared", out.cleared, "box", out.box);
await b.close();
