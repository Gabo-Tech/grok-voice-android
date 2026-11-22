import { writeFileSync } from "node:fs";

const port = 9222;
const sizes = [
  [390, 844],
  [844, 390],
  [768, 1024],
];
const modes = [
  ["cat", "flow-full"],
  ["entry", "flow-full&flow=entry"],
  ["empty", "flow-empty"],
];
const shots = sizes.flatMap(([width, height]) => modes.map(([mode, query]) => [`sankey-${mode}-${width}x${height}`, width, height, query]));

function send(ws, id, method, params) {
  ws.send(JSON.stringify({ id, method, params }));
}

async function shot(browser, name, width, height, query) {
  const page = await (await fetch(`http://127.0.0.1:${port}/json/new?${encodeURIComponent("about:blank")}`, { method: "PUT" })).json();
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve) => ws.addEventListener("open", resolve));
  const pending = new Map();
  let seq = 0;
  ws.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) pending.get(message.id)(message.result);
  });
  const call = (method, params = {}) => new Promise((resolve) => {
    const id = ++seq;
    pending.set(id, (result) => {
      pending.delete(id);
      resolve(result);
    });
    send(ws, id, method, params);
  });
  await call("Page.enable");
  await call("Runtime.enable");
  await call("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: width < 500 });
  const url = `http://127.0.0.1:4173/?shot=${query}&appearance=graphite`;
  await call("Page.navigate", { url });
  let audit = null;
  for (let i = 0; i < 40; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 250));
    const result = await call("Runtime.evaluate", {
      expression: "document.querySelector('[data-shot=money-flow]')?.dataset.labelAudit ? window.__sankeyAudit : null",
      returnByValue: true,
    });
    audit = result?.result?.value;
    if (audit) break;
  }
  const metrics = await call("Page.getLayoutMetrics");
  const content = metrics?.cssContentSize || { width, height };
  const image = await call("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: true,
    clip: { x: 0, y: 0, width: Math.max(width, content.width), height: Math.max(height, content.height), scale: 1 },
  });
  writeFileSync(`/opt/cursor/artifacts/${name}.png`, Buffer.from(image.data, "base64"));
  ws.close();
  await fetch(`http://127.0.0.1:${port}/json/close/${page.id}`);
  return { name, audit };
}

const browser = await fetch(`http://127.0.0.1:${port}/json/version`).then((response) => response.json());
if (!browser.webSocketDebuggerUrl) throw new Error("chrome debug port is not up");
const reports = [];
for (const [name, width, height, query] of shots) {
  const row = await shot(browser, name, width, height, query);
  reports.push(row);
  const ok = row.audit?.ok === true;
  console.log(`${ok ? "PASS" : "FAIL"} ${name} count=${row.audit?.count ?? 0}`);
  for (const issue of row.audit?.issues?.slice(0, 8) || ["no audit"]) console.log(`  ${issue}`);
}
if (reports.some((row) => row.audit?.ok !== true)) process.exitCode = 1;
