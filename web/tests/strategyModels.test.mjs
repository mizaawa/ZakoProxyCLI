import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { selectedStrategyModels, toggleStrategyModel } from "../src/strategyModels.ts";

const option = (model, selected = false, supported = true) => ({
  model, selected, profile_id: supported ? `profile-${model}` : null,
  profile_name: null, profile_source: null, model_family: null,
});
const options = [option("glm-5.3", true), option("claude-new"), option("unknown", false, false)];

test("opening a key restores exactly its saved model selections", () => {
  assert.deepEqual(selectedStrategyModels(options, null), ["glm-5.3"]);
  assert.deepEqual(selectedStrategyModels(options.map((item) => ({ ...item, selected: false })), null), []);
});

test("checking models does not require a template and never adds unsupported models", () => {
  const result = toggleStrategyModel(options, ["glm-5.3"], "claude-new", true);
  assert.deepEqual(result, ["glm-5.3", "claude-new"]);
  assert.deepEqual(toggleStrategyModel(options, result, "claude-new", true), result);
  assert.deepEqual(toggleStrategyModel(options, result, "unknown", true), result);
});

test("deselection is explicit, including all-off and sibling aliases", () => {
  assert.deepEqual(toggleStrategyModel(options, ["glm-5.3"], "glm-5.3", false), []);
  assert.deepEqual(selectedStrategyModels(options, []), []);
  const aliases = [option("deepseek-4.1-flash", true), option("deepseek-v4.1-flash", true)];
  assert.deepEqual(toggleStrategyModel(aliases, selectedStrategyModels(aliases, null), "deepseek-v4.1-flash", false), ["deepseek-4.1-flash"]);
});

test("group changes retain only supported selected models without enabling others", () => {
  assert.deepEqual(selectedStrategyModels([option("claude-new"), option("grok-new", true)], ["glm-5.3", "claude-new"]), ["claude-new"]);
  assert.deepEqual(selectedStrategyModels([], ["glm-5.3"]), []);
});

test("key editor has no template picker, generated bindings or template API dependency", async () => {
  const source = await readFile(new URL("../src/Management.tsx", import.meta.url), "utf8");
  for (const removed of ["template-picker", "applyTemplate", "/profile-templates", "templateId", "coldcoffee@"])
    assert.ok(!source.includes(removed), removed);
  assert.ok(source.includes("strategy_models: selectedModels"));
  assert.ok(source.includes("submitDisabled={!selectedGroup || !optionsReady}"));
});

test("model names retain a separate small strategy-source label", async () => {
  const source = await readFile(new URL("../src/Management.tsx", import.meta.url), "utf8");
  assert.match(source, /<strong>\{item\.model\}<\/strong>\s*<small>\{item\.profile_id \? `来源：\$\{item\.profile_source/);
});
