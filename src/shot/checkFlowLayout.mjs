// Rendered layout check. Needs the dev server on 127.0.0.1:4173 and Chrome on 9222.
// For every node, |length - sum(in)| < 1px and |length - sum(out)| < 1px.
// A link into a single-link node has the same width at both ends.

const pages = [
  ["vertical-d1", 390, 844, "portraitLayout=vertical&data=d1"],
  ["horizontal-d1", 390, 844, "portraitLayout=horizontal&data=d1"],
  ["landscape-d1", 844, 390, "portraitLayout=horizontal&data=d1"],
  ["vertical-d3", 390, 844, "portraitLayout=vertical&data=d3"],
  ["horizontal-d3", 390, 844, "portraitLayout=horizontal&data=d3"],
  ["landscape-d3", 844, 390, "portraitLayout=horizontal&data=d3"],
];

const created = await fetch("http://127.0.0.1:9222/json/new?about:blank", { method: "PUT" }).then((res) => res.json());
const ws = new WebSocket(created.webSocketDebuggerUrl);
await new Promise((resolve) => { ws.addEventListener("open", resolve); });
let seq = 0;
const waiters = new Map();
ws.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.id && waiters.has(message.id)) {
    waiters.get(message.id)(message);
    waiters.delete(message.id);
  }
});
const send = (method, params = {}) => new Promise((resolve) => {
  const id = ++seq;
  waiters.set(id, resolve);
  ws.send(JSON.stringify({ id, method, params }));
});
await send("Page.enable");
await send("Runtime.enable");

const expression = `(() => {
  const chart = window.__flowChart;
  const audit = document.querySelector('[data-shot=money-flow]')?.dataset.labelAudit || "";
  if (!chart || !audit) return JSON.stringify(null);
  const series = chart.getModel().getSeriesByIndex(0);
  const orient = series.get('orient');
  const graph = series.getGraph();
  const failures = [];
  graph.eachNode((node) => {
    const layout = node.getLayout();
    const length = orient === 'vertical' ? layout.dx : layout.dy;
    let inn = 0;
    let out = 0;
    node.inEdges.forEach((edge) => { inn += edge.getLayout().dy || 0; });
    node.outEdges.forEach((edge) => { out += edge.getLayout().dy || 0; });
    if (node.inEdges.length && Math.abs(length - inn) >= 1) failures.push(node.id + ' in ' + inn.toFixed(2) + ' len ' + length.toFixed(2));
    if (node.outEdges.length && Math.abs(length - out) >= 1) failures.push(node.id + ' out ' + out.toFixed(2) + ' len ' + length.toFixed(2));
  });
  graph.eachEdge((edge) => {
    const width = edge.getLayout().dy || 0;
    const sourceLength = orient === 'vertical' ? edge.node1.getLayout().dx : edge.node1.getLayout().dy;
    const targetLength = orient === 'vertical' ? edge.node2.getLayout().dx : edge.node2.getLayout().dy;
    const sourceShare = edge.node1.outEdges.length === 1 ? sourceLength : width;
    const targetShare = edge.node2.inEdges.length === 1 ? targetLength : width;
    if (Math.abs(sourceShare - width) >= 1 || Math.abs(targetShare - width) >= 1) {
      failures.push('edge ' + width.toFixed(2) + ' src ' + sourceShare.toFixed(2) + ' dst ' + targetShare.toFixed(2));
    }
  });
  return JSON.stringify({ audit, orient, failures: failures.slice(0, 8), count: failures.length });
})()`;

let failed = false;
for (const [name, width, height, query] of pages) {
  await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: true });
  await send("Page.navigate", { url: `http://127.0.0.1:4173/?shot=flow-full&appearance=graphite&flow=category&labels=1&options=closed&${query}` });
  let value = null;
  for (let attempt = 0; attempt < 40; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 250));
    const result = await send("Runtime.evaluate", { expression, returnByValue: true });
    value = JSON.parse(result.result?.result?.value || "null");
    if (value) break;
  }
  if (!value || value.count !== 0) failed = true;
  console.log(name, value ? `${value.orient} audit=${value.audit} failures=${value.count}` : "no chart");
}
ws.close();
process.exit(failed ? 1 : 0);
