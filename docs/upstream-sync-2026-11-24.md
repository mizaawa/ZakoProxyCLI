# GPT 上游策略逐项对比 · 2026-11-24

## 比较范围与结论

- 本项目更新前版本：v0.14.2，提交 `ca76523`。实际专用策略来源：`86997bffc0355930918c82311e2c34aa04d2b5b8`。
- 独立 GPT 导入器旧 pin：`3249afce6b853ff2f2133e016b223db2cf38b087`。
- 上游最新 main：`a30126e7c7d6c2ffe6b08f007cbc055d9b498f71`，提交时间 2026-11-23 21:10:07 +08:00；工作台版本 3.3.0-preview.5。

**GPT-6.2 确实是上游本次着重同步和展示的型号，但“着重同步型号”不等于“新增三套破甲规则”。** 对两个旧 pin 分别与最新提交进行 Git blob 比较，GPT 相关的 96 个正文/拼装文件全部相同：包含 Codex 主模板、工作流、激活页、共享路由/80 个叶子、`seat-packs.js`、IDA/MCP、unified-work 和 ChatGPT Dot 模板。变化发生在 prompt-engine 席位 metadata、产品名及模型推荐代码等处。

未运行上游安装器、JavaScript 或技能；只将固定版本代码/文本作为数据读取与比较。未声称验证闭源、收费服务端可能使用的策略；其内容无法由公开仓库推断。

## 三个 GPT 型号分别有什么变化

| 项目 | GPT-6.2 Astra | GPT-6 Astra | GPT-6.2 Sol |
| --- | --- | --- | --- |
| 上游独立提示词文件 | 无，使用共享 `codex.md` | 同左 | 同左 |
| 原始主模板/工作流/路由 | 未改 | 未改 | 未改 |
| prompt-engine 席位 | 从旧 GPT-6 标识改为 GPT-6.2 Astra / Sol | 同一个 Codex 席位仍保留兼容 | 同 Astra 6.2 |
| 推荐排序 rank | 0 → 0 | 2 → 5（新增三类优先项后下移） | 1 → 1 |
| 本项目原策略 ID | `coldcoffee-gpt-6.2-astra-v1` | `gpt-6-astra-v2-rc1` | `coldcoffee-gpt-6.2-sol-v1` |
| 本次处理 | 保留 ID，最新 pin 重新生成 | 保留 ID，最新 pin 重新生成 | 保留 ID，最新 pin 重新生成 |

`rank` 是上游推荐顺序，不是模型性能、能力或提示效果评分。识别逻辑仍支持 provider 前缀、忽略显示匹配中的大小写/分隔符，并保留服务商返回的实际模型 ID。本项目策略匹配保持严格准确 ID，不继承宽松显示匹配为注入规则。

## 哪些代码真的变了

1. `desktop/src/lib/prompt-engine.js`：`APP_TITLE` 加入 GPT-6.2 Astra、GPT-6.2 Sol 等最新显示名；Codex 的 `tag` 从 `GPT-6 Astra全模型支持` 改为 `GPT-6.2 Astra / Sol`。未改变 Codex 工作流或任务规则。
2. `desktop/src/shared/workbench-core.js`：Codex 与 ChatGPT Dot 的 GPT-6.2 标识在旧运行 pin 中已经存在，本次并非首次引入。此次主要更新其他席位显示名并新增 OpenCode。
3. `desktop/src/shared/relay-models.js`：将固定两类 GPT 匹配扩展为带 `rank/match` 的五类目录；增加 GLM-5.4、Claude Fable 5.2、Gemini 4。6.2 Astra / Sol 仍为前两名，6 Astra 兼容项继续保留。
4. `desktop/src/lib/beginner-install.js`：Codex 安装卡片副标题从 GPT 改为 GPT-6.2，属于安装入口标识更新。
5. OpenCode 的安装、配置、环境隔离及模型上下文传递是客户端接入变化，不是三种 GPT 的专属提示词变化。

## 可复核的字节证据

以下原始文件在两个旧 pin 与最新 pin 中 Git blob 完全一致；SHA-256 按 Git 原始 LF 字节计算。

| 上游路径 | 字节数 | SHA-256 |
| --- | ---: | --- |
| `desktop/src/lib/packs/codex.md` | 2402 | `bd75d0241cd8c57a1778feb93e906385bf493f231c2fe9e72dbf8ab387304f07` |
| `desktop/src/lib/packs/workflows/codex.md` | 732 | `e43111915b7444735a3ab80ae3f3a307118c0580e0ce967f0b936284f0923011` |
| `desktop/src/lib/packs/activation-reply.md` | 1832 | `09ea8e92a255506fb9ae33f7f970ea1cef37a4f36e35f9aae729ef8cbaf9f784` |
| `desktop/src/lib/seat-packs.js` | 4264 | `282708c02e7a1bfbfd5294d2a59b77f3b72157385aed3fb808a7af05ee9c2945` |
| `desktop/src/lib/packs/local-agent-skills/chatgpt-dot.md` | 837 | `dc1b08acaac8e63152e9baab5dd12bcc9e5a3c28decf4470d153e11e8d2c7b43` |

复核命令（在上游仓库运行）：

```sh
git diff 86997bffc0355930918c82311e2c34aa04d2b5b8 a30126e7c7d6c2ffe6b08f007cbc055d9b498f71 -- desktop/src/lib/packs/codex.md desktop/src/lib/packs/workflows/codex.md desktop/src/lib/packs/routes desktop/src/lib/seat-packs.js
# 上述范围为空 diff。
git diff 86997bffc0355930918c82311e2c34aa04d2b5b8 a30126e7c7d6c2ffe6b08f007cbc055d9b498f71 -- desktop/src/lib/prompt-engine.js desktop/src/shared/relay-models.js
```

## 本项目不是“仅改个来源版本号”

- 移除 GPT 必须保留旧载荷逐字不变的导入约束，独立 GPT 和全量导入器共用同一最新 pin 与 metadata 解析。
- 所有模板重新读取上游原始文件生成；按上游实际差异，本地结果为 7 个正文更新、8 个原有正文不变、1 个新增 OpenCode。
- 上游原始 `codex.md` 的静态 seat 行仍写 GPT-6，而 prompt-engine 已使用 GPT-6.2。适配器只将该显示行与最新 metadata 对齐，并在 provenance 中明确记录；不将本地这项适配伪称为上游新任务规则。
- 生成后的 GPT 文本为 65,582 UTF-8 字节，SHA-256：`53fa5d080edd3ad7c1cfdb62bf5d9d5d7ba30cfb038f33c168d75216c2b00d69`。全部五个现有 GPT 实际模型 ID 都引用同一最新正文。
- 仍有 API 能力边界说明，移除安装器专用块；不安装技能，不捏造工具/验证结果，不自动启用策略。通用默认兜底策略不是上游按型号模板，本次保留。
- 新增模型与 OpenCode、兼容性及回滚见 [v0.14.3 发布说明](releases/v0.14.3.md)。结构和协议回归检查不能证明真实模型效果。
