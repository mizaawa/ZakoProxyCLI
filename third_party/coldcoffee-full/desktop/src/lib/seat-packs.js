const fs = require("node:fs");
const path = require("node:path");
const { APP_TITLE, ACTIVATION_WORD, CONTROL_WORD, SEATS, activationReply } = require("./prompt-engine");

const PACK_IDS = ["codex", "claude", "grok", "deepseek", "glm53", "gemini", "doubao", "workbuddy", "cursor", "mimo", "kimi"];
const ROUTE_IDS = [
  "cha-bin-unlock",
  "cha-playfield",
  "cha-netcast",
  "cha-apicloud",
  "cha-labpipe",
  "cha-inkstage",
];

function packsDir() {
  const here = path.join(__dirname, "packs");
  if (fs.existsSync(here)) return here;
  const fromResources = process.resourcesPath ? path.join(process.resourcesPath, "packs") : null;
  if (fromResources && fs.existsSync(fromResources)) return fromResources;
  return here;
}

function seatMeta(seatId) {
  return SEATS.find((item) => item.id === seatId) || null;
}

function wrapMarked(seatId, body) {
  const begin = `<!-- CHA-${seatId.toUpperCase()}-POJIA:BEGIN -->`;
  const end = `<!-- CHA-${seatId.toUpperCase()}-POJIA:END -->`;
  const trimmed = String(body).replace(/\s+$/, "");
  if (trimmed.includes(begin) && trimmed.includes(end)) return trimmed.endsWith("\n") ? trimmed : `${trimmed}\n`;
  return `${begin}\n${trimmed}\n${end}\n`;
}

function renderRouteSkill(routeId) {
  const file = path.join(packsDir(), "routes", `${routeId}.md`);
  if (!fs.existsSync(file)) throw new Error(`missing route: ${routeId}`);
  return fs.readFileSync(file, "utf8").replace(/\s+$/, "") + "\n";
}

function leafIds() {
  const dir = path.join(packsDir(), "routes", "leaves");
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir).filter((name) => name.endsWith(".md") && name !== "INDEX.md").map((name) => name.replace(/\.md$/, "")).sort();
}

function renderLeafSkill(leafId) {
  return fs.readFileSync(path.join(packsDir(), "routes", "leaves", `${leafId}.md`), "utf8").replace(/\s+$/, "") + "\n";
}

function renderRouter() {
  const head = fs.readFileSync(path.join(packsDir(), "routes", "ROUTER.md"), "utf8").replace(/\s+$/, "");
  const talkFile = path.join(packsDir(), "routes", "NORMALIZER.md");
  const talk = fs.existsSync(talkFile) ? fs.readFileSync(talkFile, "utf8").replace(/\s+$/, "") : "";
  const parents = ROUTE_IDS.map((id) => renderRouteSkill(id).replace(/\s+$/, "")).join("\n\n");
  const indexFile = path.join(packsDir(), "routes", "leaves", "INDEX.md");
  const index = fs.existsSync(indexFile) ? fs.readFileSync(indexFile, "utf8").replace(/\s+$/, "") : "";
  const leaves = leafIds().map((id) => renderLeafSkill(id).replace(/\s+$/, "")).join("\n\n");
  return [head, talk, parents, index, leaves].filter(Boolean).join("\n\n") + "\n";
}

function idaMcpBlock() {
  return fs.readFileSync(path.join(packsDir(), "ida-mcp.md"), "utf8").replace(/\s+$/, "");
}

function renderPack(seatId) {
  const file = path.join(packsDir(), `${seatId}.md`);
  if (!fs.existsSync(file)) throw new Error(`missing pack: ${seatId}`);
  const seat = seatMeta(seatId);
  const raw = fs.readFileSync(file, "utf8");
  const workflowFile = path.join(packsDir(), "workflows", `${seatId}.md`);
  const workflow = fs.existsSync(workflowFile) ? fs.readFileSync(workflowFile, "utf8").replace(/\s+$/, "") : "";
  const text = raw
    .replaceAll("{{APP_TITLE}}", APP_TITLE)
    .replaceAll("{{ACTIVATION_WORD}}", ACTIVATION_WORD)
    .replaceAll("{{CONTROL_WORD}}", CONTROL_WORD)
    .replaceAll("{{SEAT_ID}}", seatId)
    .replaceAll("{{SEAT_TAG}}", seat?.tag || seatId)
    .replaceAll("{{SEAT_NAME}}", seat?.name || seatId)
    .replaceAll("{{ACTIVATION_REPLY}}", activationReply().replace(/\s+$/, ""))
    .replaceAll("{{WORKFLOW}}", workflow)
    .replaceAll("{{IDA_MCP}}", idaMcpBlock())
    .replaceAll("{{UNIFIED}}", fs.readFileSync(path.join(packsDir(), "unified-work.md"), "utf8").replace(/\s+$/, ""))
    .replaceAll("{{ROUTER}}", renderRouter().replace(/\s+$/, ""));
  return wrapMarked(seatId, text);
}

function markers(seatId) {
  return {
    begin: `<!-- CHA-${seatId.toUpperCase()}-POJIA:BEGIN -->`,
    end: `<!-- CHA-${seatId.toUpperCase()}-POJIA:END -->`,
  };
}

module.exports = {
  PACK_IDS,
  ROUTE_IDS,
  packsDir,
  renderPack,
  renderRouter,
  renderRouteSkill,
  renderLeafSkill,
  leafIds,
  markers,
  seatMeta,
  APP_TITLE,
  ACTIVATION_WORD,
  CONTROL_WORD,
  activationReply,
};
