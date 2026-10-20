# ZakoAI Proxy

简体中文 · [English](README.en.md) · [安全说明](SECURITY.md) · [API 契约](API_CONTRACT.md)

基于 Rust 的自托管 Responses / Chat Completions / Anthropic Messages API 中转，提供适配 PC 和移动端的管理控制台、SQLite 持久化和 Docker Compose 部署。侧边栏包含仪表盘、分组管理、账号管理、密钥管理、调用日志和系统设置。

## ⚠️ 免责声明 · Disclaimer

### 简体中文

> [!WARNING]
> **严禁将本项目用于任何违法违纪活动。**

本项目仅供合法、合规的学习、研究和在你本人有权操作的环境里使用。包括但不限于：未经授权的入侵、攻击、渗透、窃取数据；诈骗、洗钱、传播恶意程序；生成或传播违法有害内容；侵犯他人隐私、知识产权和其他合法权益；违反所在地法律法规或第三方平台、模型服务的使用条款。

使用者须自行遵守所在国家和地区的法律法规，以及相关软件和服务的使用条款。使用本项目产生的一切行为和后果，由使用者自行承担，与项目作者及贡献者无关。如果你不同意以上内容，请不要下载、安装或使用本项目。

### English

> [!WARNING]
> **This project must NOT be used for any illegal or unlawful activity.**

It is provided only for lawful and compliant learning, research, and use in environments you are authorized to operate. Prohibited uses include, without limitation: unauthorized intrusion, attacks, penetration, or data theft; fraud, money laundering, or distributing malware; generating or spreading illegal or harmful content; infringing others' privacy, intellectual property, or other legal rights; and violating applicable laws or the terms of use of third-party platforms and model services.

You are solely responsible for complying with the laws and regulations of your jurisdiction and with the terms of the software and services you use. All actions and consequences arising from the use of this project are borne entirely by the user, and the authors and contributors accept no liability. If you do not agree, do not download, install, or use this project.

---

# AI中转站推荐 -- 杂鱼API

[点我跳转](https://www.zayuapi.com/)

*稳定不掺水，已注册活跃用户高达 3000+，自有私有原创反代协议号池，支持 GPT、Claude、Gemini、Grok 外加大部分国模，价格始终处于国内第一低价梯队。*

## 小白向教程
博客文档：https://www.mizaawa.com/index.php/archives/95/

## Docker Compose 部署

需要 Docker Engine / Docker Desktop 和 Docker Compose v2：

```sh
git clone https://github.com/mizaawa/ZakoProxyCLI.git
cd ZakoProxyCLI
docker compose pull
docker compose build --pull
docker compose up -d --no-build
docker compose ps
```

访问 **http://localhost:1990**。全新数据卷首次进入会创建管理员账号，密码至少 12 个字符；之后使用同一账密登录。项目没有默认管理员账号或密码，请先完成初始化再对外开放服务。

`pull` 下载已发布镜像；`build --pull` 更新 Node、Rust、Debian 基础镜像并从当前源码重新编译项目；`up -d` 启动服务。镜像名称为 `ghcr.io/mizaawa/zakoai-proxy-cli:latest`，默认分支 CI 成功后会发布该镜像及对应提交 SHA 标签。

Compose 默认仅将宿主机 **127.0.0.1:1990** 映射到容器。需要对外开放端口时设置 `ZAKO_BIND_IP=0.0.0.0`，`ZAKO_PORT` 可修改宿主机端口。可在不提交 Git 的 `.env` 中配置：

```dotenv
ZAKO_BIND_IP=127.0.0.1
ZAKO_PORT=1990
TRUST_PROXY=0
```

公网部署请通过可信反向代理提供 HTTPS，后端端口保持私有。仅在此代理之后设置 `TRUST_PROXY=1`，代理须保留原始 `Host` 并传入 `X-Forwarded-Proto: https`，从而启用会话 Cookie 的 `Secure` 属性。SSE 代理需关闭响应缓冲，读取超时应大于系统设置中的请求超时。

```sh
docker compose logs --tail=100 -f
curl --fail http://localhost:1990/health
docker compose down
```

`down` 保留数据卷，`down -v` 会删除管理员、配置、密钥和统计数据。

### 在控制台更新

v0.5.0 起，登录后自动检查新版本，并在页面可见时每 15 分钟复查；后台标签暂停检查，退出登录停止检查。发现更高的正式版本时，页面标题旁显示带圆点的黄色“有新版本”标签，点击进入系统设置并定位更新区域。手动检查与顶部提示同步。查询在服务端合并并发请求，成功缓存 5 分钟，失败退避 1 分钟；未配置更新器也可检查和提示，执行构建更新仍需配置更新器。

v0.3.0 起可在 **系统设置 → GitHub 源码更新** 中检查 GitHub 最新正式 Release。选择更新后需再次输入当前管理员密码；更新器从官方仓库检出对应标签，在后台从源码构建镜像，并显示构建、部署和健康检查日志。构建期间旧版本继续服务，切换版本时会短暂中断连接；页面恢复后可继续查看结果。

Compose 同时启动隔离的 `updater` 服务，不对宿主机发布端口。主程序通过内部 `UPDATER_URL=http://updater:9000/` 及只读 `/control/token` 访问它。Docker socket 仅挂载给更新器，不挂载给公开的代理服务。更新器持有近似宿主机 root 的 Docker 控制能力，请仅在受控服务器上部署，不要对外暴露内部接口或共享令牌。

源码构建成功后，更新器先停止代理并备份完整数据，再启动新镜像；部署或健康检查失败时恢复旧版本及同一次数据备份。更新状态和备份保留在 `update-state` 数据卷中，不能代替异地备份。账号、API key、分组、统计和 `master.key` 随数据卷保留。更新只接受官方仓库的正式版本标签，不支持输入任意地址或执行命令。

旧版部署需先拉取本版本源码并执行一次上述 Compose 构建和启动命令，以安装更新器。使用自定义 Compose 项目名时，保持 `COMPOSE_PROJECT_NAME` 一致；不使用 Compose 或未配置更新器的本地开发环境只展示版本信息，不能在网页执行更新。更新代理镜像不会自行替换更新器或修改宿主机 Compose 文件；发布说明要求调整部署配置时仍需由管理员执行。

### v0.10.0 樱花布丁主题

控制台界面整体改为二次元风格的“樱花布丁”主题：以樱花粉、暖蜂蜜黄和奶油白为主色，卡片、按钮、输入框和弹窗采用更大的圆角；侧边栏和顶栏改为浮动卡片，系统设置与调用日志改为卡片布局，图表使用柔和的渐变填充，登录页和空状态新增“杂鱼酱”小鱼插画。界面西文字体由 Inter 改为 Nunito（OFL-1.1，随前端本地打包），中文继续使用系统字体。本版本只修改前端外观，API、数据与部署方式不变，升级无需额外操作。

### v0.9.0 Turnstile 安全校验与日志统计改进

系统设置支持配置 Cloudflare Turnstile 站点密钥和密钥，登录时启用人机校验；密钥加密保存且不会通过 API 返回。调用日志分页器支持键盘操作和移动端触控，仪表盘对大数量统计使用紧凑显示；新增超过千条 Cyber 事件的统计覆盖测试。

### v0.9.3 Turnstile `.env` 配置与 Release 同步

Turnstile 站点密钥和密钥统一保存在项目根目录 `.env`，系统设置保存时会自动更新该文件；旧数据库中的 Turnstile 配置会被清理。系统设置提供后台连接测试和右上角 Toast 提示。GitHub 源码更新以最新正式 Release 为准；当本地版本高于 Release 时，更新按钮会同步到该正式版本。

### v0.8.0 DeepSeek API 兼容策略

为全部五个 DeepSeek 精确模型档案组合一层共享的 API 适配提示，吸收 dsh-purge 的请求期提示组装与上下文连续性思路；策略仍按映射后的实际模型选择，并沿用现有 Responses / Compact、Chat Completions 和 Anthropic 注入链路。原 profile ID 与旧版配置识别规则保留，模型目录数量不变。适配层以当前请求实际携带的消息、工具和结果为准，不假设 Harness 本地文件、hook、审批、沙箱、跨请求记忆或权限，也不声称修改上游模型策略。来源版本、改编范围和 MIT 声明见 [profiles/dsh-purge/SOURCE.json](profiles/dsh-purge/SOURCE.json) 与 [third_party/dsh-purge/NOTICE.md](third_party/dsh-purge/NOTICE.md)。

### v0.7.0 调用日志与请求链路修复

密钥管理下新增独立调用日志，默认保留最近 7 天，每页 20 条，支持分页跳页、JSON 导出、清空日志、状态码和结构化 JSON/SSE 错误详情，并记录首字耗时及缓存读写 Token；仪表盘不再重复展示调用日志。Responses / Chat 流式工具请求现在在确认终止事件后及时释放并发槽；正常的长度限制或内容过滤不会误判为账号故障。账号冷却返回带真实剩余时间的 503，RPM 和并发本地限制保留 429，并提供机器可读错误原因。真实上游的 HTTP 状态、脱敏错误正文及 Retry-After 继续透传。

### v0.6.0 仓库改名后的首次升级

官方仓库已改名为 ZakoProxyCLI。v0.5.0 及更早版本的更新器严格匹配旧 Release URL，因此仓库改名后网页检查或更新可能失败。请在原部署目录执行一次以下操作，同时替换 proxy 和 updater；保持原目录、Compose 项目名和数据卷不变。若本地修改导致拉取失败，请先处理冲突。

```sh
git remote set-url origin https://github.com/mizaawa/ZakoProxyCLI.git
git pull --ff-only origin main
docker compose build --pull
docker compose up -d --no-build
docker compose ps
```

镜像仍发布至 ghcr.io/mizaawa/zakoai-proxy-cli，保持现有部署兼容。v0.6.0 识别新旧官方 Release URL；本次首次迁移不能依赖旧更新器自动完成。

## 配置中转

1. **分组管理：** 创建分组，在“破甲策略”中通过 `profile_ids` 独立勾选专用策略。目录共有 36 个策略、37 个精确模型 ID（DeepSeek 4.1 Flash 的两个历史拼写共用原策略，`deepseek-flash` 作为独立 Hanshuang 策略），完整名单见下表。另有“开启破甲默认策略”开关 `default_strategy_enabled`，新建及旧版分组默认关闭。按账号映射后的实际模型匹配已选专用策略，命中时优先使用；未命中时，仅开关开启才使用默认策略。每次请求最多使用一种策略，不拼接专用与默认正文。分组不配置自定义模型列表或指令；账号可用模型、协议和分组资格仍独立检查，开关不会增加可调用模型。旧版未修改过的内置模板分组会自动识别。删除分组前需先解除账号和密钥引用。
2. **账号管理：** 输入上游端点及 API key，“支持协议”默认勾选 `responses` / `compact` / `chat` / `anthropic`，可按上游中转站支持情况取消，再拉取并勾选上游实际可用模型、选择所属分组。`chat` 对应 `/v1/chat/completions`，`anthropic` 对应 `/v1/messages`；选择 Anthropic 获取模型时也使用 Anthropic 鉴权。模型名称不限定协议：例如 Grok、Gemini、DeepSeek 或 GLM 可以走中转站提供的 OpenAI 兼容或 Anthropic 兼容接口。可通过 `model_mappings` 设置“请求模型 → 实际模型”，映射目标必须已勾选；映射仅执行一次，不递归串联，未配置映射的请求使用同名模型。端点可带或不带 `/v1`。RPM 和并发限制设为 `0` 表示不限制；编辑时密钥留空会保留原密钥。“模型映射”和“所属分组”之间可选填账号代理，支持 `socks5://host:port`、`socks5h://host:port` 和带用户名密码的链接（特殊字符需百分号编码）。右侧测试按钮通过该代理请求当前上游模型接口，显示耗时、HTTP 状态或脱敏错误；HTTP 401/403 表示代理已连通但上游鉴权失败。代理地址加密保存，编辑留空保留，点击“移除代理”并保存才会删除。保存后四协议、流式请求及模型获取均走该账号代理，代理失败不会自动改为该账号直连；重试使用下一个账号自己的代理。Docker 中 `127.0.0.1` 指容器自身，应填写容器可访问的代理地址。
3. **密钥管理：** 为指定分组创建命名的客户端 API key，可在创建后及之后登录控制台时复制。修改所属分组无需更换 key；停用或删除后阻止新的请求。API 基础地址为当前站点来源加 `/v1`。
4. **系统设置：** 设置安全入口、最大重试数、失败阈值、冷却时间、请求超时及统计保留天数。修改密码需验证原密码并再次确认新密码，成功后撤销所有会话。此处二次确认不是 TOTP 等独立第二认证因素。

例如，账号配置片段 `{"models":["gpt-6-astra"],"model_mappings":{"chat-main":"gpt-6-astra"}}` 允许客户端请求 `chat-main`，向上游发送的 `model` 为 `gpt-6-astra`；分组勾选 `gpt-6-astra` 策略后，注入该模型的策略，而不是按别名匹配。未设置映射的 `gpt-6-astra` 请求仍使用同名实际模型。所有支持协议使用相同的模型映射与策略匹配规则。

### 内置策略模型

默认策略从现有 `gpt-6-astra` 提示词的任务执行与连续任务机制改编为多模型 API 文本，见 [profiles/default.md](profiles/default.md) 和 [profiles/default.SOURCE.json](profiles/default.SOURCE.json)。正文为 3,844 字节 UTF-8，移除固定模型身份、本机工具环境和强制四文件交付等假设，保留客户端上下文和真实可用工具。开启后，未选专用策略的模型及目录外模型均可使用默认策略，前提仍是账号已勾选该实际模型。界面提示：“开启后，未勾选模型将采取默认提示词注入尝试进行破甲，如造成无法正常使用现象请关闭此选项”。分组 API 的 `PUT` 省略开关字段保留原值，显式 `false` 关闭；`null` 或非布尔值会被拒绝。

目前 27 个策略继续使用冷咖啡正文，保留上游席位正文、工作链和完整路由技能，单次匹配注入约 64–66 KB 的 UTF-8 文本，会占用上下文并增加输入 token。v0.6.0 将 DeepSeek 4.1 Flash 的一个策略替换为寒霜改编正文，其余 DeepSeek 策略不变。冷咖啡历史来源、逐文件哈希及适配范围记录于 [profiles/coldcoffee/SOURCE.json](profiles/coldcoffee/SOURCE.json)；可用 `python scripts/import-coldcoffee.py UPSTREAM_CHECKOUT` 从指定版本的干净上游副本重新生成，脚本只读取数据，不执行安装器或 Skill。

| 模型系列 | 精确匹配的实际模型 ID |
| --- | --- |
| GPT（保留） | `gpt-5.6-sol`、`gpt-6.1-sol`、`gpt-6-astra` |
| Claude | `claude-fable-5`、`claude-fable-5-1`、`claude-haiku-4-5`、`claude-haiku-5-5`、`claude-opus-4-5`、`claude-opus-4-6`、`claude-opus-4-7`、`claude-opus-4-8`、`claude-opus-5`、`claude-opus-5-5`、`claude-sonnet-4-5`、`claude-sonnet-4-6`、`claude-sonnet-5`、`claude-sonnet-5-5` |
| Grok | `grok-4.7` |
| Gemini | `gemini-3-flash`、`gemini-3.1-flash-image`、`gemini-3.1-pro`、`gemini-3.1-pro-preview`、`gemini-3.6-flash`、`gemini-3.7-flash`、`gemini-3.8-flash` |
| DeepSeek | `deepseek-v4-flash`、`deepseek-v4-flash-0731`、`deepseek-v4-pro`、`deepseek-v4-pro-0813`、`deepseek-v4.1-flash` / `deepseek-4.1-flash` / `deepseek-flash`（`deepseek-v4.1-flash` 与 `deepseek-4.1-flash` 共用原寒霜策略，`deepseek-flash` 为独立寒霜策略） |
| GLM | `glm-5.3` |
| Kimi | `kimi-k3`、`kimi-k2.7`、`kimi-k2.7-code`、`kimi-k2.6` |

专用策略按精确模型选择，不按系列通配匹配。寒霜策略沿用 `coldcoffee-deepseek-v4.1-flash-v1`，已有分组选择继续匹配 `deepseek-v4.1-flash` 与 `deepseek-4.1-flash`；新增独立 `hanshuang-deepseek-flash-v1` 策略匹配 `deepseek-flash`，正文相同。名单表示策略选择，并不承诺上游提供这些模型；以账号实际可用模型和协议为准。冷咖啡、寒霜和 Kimi 正文均为内联请求文本，代理不会安装本地 Skill、工具或客户端配置，也不会伪造工具执行结果。实际模型行为须在使用的中转站验证。

调度仅选择匹配当前分组、协议且映射后实际模型已勾选的可用账号，未勾选任何可用模型的账号不接收请求。**优先级数字越小越先尝试**，相同优先级按账号 ID 稳定排序。单请求最多尝试 `最大重试数 + 1` 个不同账号。每次尝试都从原始客户端请求重新应用当前账号的单跳映射，再按实际模型匹配分组策略；故障转移到其他账号时独立计算，不继承或叠加前一账号的策略。连接失败、HTTP 401、403、408、429 和 HTTP 5xx 可在响应开始前切换到下一账号；连续可重试失败达到阈值时进入冷却，冷却时间结束后重新参与调度。其余 4xx 原样返回，不触发重试。流式响应已开始后中断不会重放请求。RPM 窗口、并发及冷却状态只保存在当前进程，重启后重置；同一个数据卷只运行一个实例。

安全入口将控制台从 `/` 移到 `/<入口>`，`/health` 和 `/v1/*` 不受影响。它是额外的入口隐藏措施，不代替登录认证。保存设置后请记录新的登录地址。

## 客户端 API

提供 `GET /v1/models`、`POST /v1/responses`、`POST /v1/responses/compact`、`POST /v1/chat/completions` 和 `POST /v1/messages`。`/v1/models` 包含当前分组账号已勾选的实际模型及有效映射的请求模型名。鉴权使用控制台生成的中转密钥，通过 `Authorization: Bearer` 或 `x-api-key` 传入。客户端与账号使用同一种请求协议，代理不转换 API 协议或补充 Google 等厂商的原生接口。

Chat Completions 客户端使用当前站点加 `/v1` 作为基础地址，并启用账号的 `chat` 协议。`/v1/chat/completions` 保留原有消息、工具字段和 SSE，按分组专用优先、默认兜底规则选出的策略作为独立的 `system` 消息前置，保留客户端原有消息及其他请求字段。JSON 和 SSE 响应沿用上游 Chat Completions 格式。

Anthropic 客户端使用当前站点作为服务地址，选择已启用 `anthropic` 协议的账号模型。`/v1/messages` 保留原生消息、工具调用与 SSE，转发 `anthropic-version`（默认 `2023-06-01`）及 `anthropic-beta`。按同一分组策略选择规则，将选出的正文前置到 `system`，保留原有字符串或文本块数组。模型映射、故障切换、限流及统计沿用相同调度规则；输入 token 包含 Anthropic 缓存读取及写入，缓存命中 token 统计读取量。

```sh
export ZAKO_API_KEY='YOUR_GENERATED_PROXY_KEY'
curl http://localhost:1990/v1/responses \
  -H "Authorization: Bearer $ZAKO_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"YOUR_REQUEST_MODEL_ID","input":"Hello"}'

curl -N http://localhost:1990/v1/responses \
  -H "Authorization: Bearer $ZAKO_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"YOUR_REQUEST_MODEL_ID","input":"Hello","stream":true}'
```

`YOUR_REQUEST_MODEL_ID` 可以是已勾选的实际模型名或账号配置的请求模型别名。上下文压缩使用 `/v1/responses/compact`，实际支持情况由上游决定。Responses 和 Compact 先解析当前账号的实际模型，再优先选择该模型已勾选的专用策略；未匹配时按 `default_strategy_enabled` 决定是否使用默认策略。选中的一种正文前置到 `instructions`，原有客户端指令和其他字段保留；开关关闭且未匹配专用策略时不注入。Chat Completions 与 Anthropic Messages 使用相同规则，分别通过独立 `system` 消息和 `system` 字段注入。SSE 转发同时读取用量和完成状态，上游凭证反射会被脱敏。“破甲”配置属于可选指令文本，不保证抢占上游系统指令优先级、改变模型行为或消除拒答，实际结果取决于上游服务。

工具调用与流结束：Responses 的 `response.completed`、Anthropic 的 `message_stop`、Chat 的 `[DONE]` 或全部 `choices` 的非空 `finish_reason` 都会释放账号并发槽，后续工具结果可立即续接；仍继续透传尾部用量。Chat 中转在完整 `finish_reason` 后正常 EOF、未发送 `[DONE]` 时也视为正常结束；仅部分 choice 结束或没有完成标志仍按截断处理。Responses 因 `max_output_tokens` / `content_filter` 正常停止不会累积账号故障，真正的错误、连接中断和截断仍触发故障冷却。

排查 429：本地 RPM / 并发超限返回 `429`，错误 `code` 分别为 `rate_limit_exceeded` / `concurrency_limit_exceeded`；账号故障冷却返回 `503` 和 `upstream_cooldown`，`Retry-After` 为剩余冷却秒数（RPM 为当前分钟剩余秒数，并发为建议 1 秒）。这些本地调度错误含 `x-zako-error-source: local` 与请求 ID。真实上游 HTTP 错误标为 `upstream`，保留状态、脱敏正文及 `Retry-After`；不会通过更换状态码掩盖上游限流。

## 统计与存储

- 仪表盘展示输入、输出、缓存及未命中 token、缓存命中率、RPM、TPM、总请求、成功率、平均耗时、每日图表、历史区间和 Cyber 标记次数。
- Token 总量、Token 分布及相关缩写显示使用 `K`（千）、`M`（百万）、`B`（十亿）、`T`（万亿），缩写固定两位小数，例如 `90.48M`、`1.00B`；不足一千保持整数。仅更改显示，API 用量整数和导出原始数据不变。
- 缓存 token 已包含在输入 token 内，总 token 为输入加输出，不重复计数。用量来自上游 JSON 或完成后的 SSE 用量字段，缺失时计为零，不估算。
- RPM / TPM 为最近 60 秒已结束并记录的请求；每日统计使用 UTC。总量和单密钥用量基于保留中的请求元数据，并非永久计费账本。默认保留 90 天，新请求写入时清理过期记录。
- Cyber 计数表示响应中出现不区分大小写的 `cyber` 文本，每个响应最多计一次。它是文本启发式统计，不等同于特定弹窗、拒答或模型行为的检测结果。
- `/data/zako.db` 保存配置、会话摘要和请求元数据。上游密钥及可再次复制的客户端密钥使用 `/data/master.key` 配合 AES-256-GCM 加密；管理员密码使用 Argon2 摘要。请求日志不持久化提示词或响应正文；SQLite 文件和其他元数据未进行整盘加密。

### 备份与恢复

请将**整个 `/data`，特别是 `master.key`** 备份到受控存储。停机时可得到一致备份：

```sh
docker compose stop
docker compose cp proxy:/data ./zako-backup
docker compose start
```

恢复时先停止服务，将备份内容放回数据卷 `/data` 根目录并保留 `10001:10001` 所有权。数据库、SQLite 附属文件和 `master.key` 应来自同一次备份；缺少匹配主密钥的数据库不能解密存储的 API key。恢复后检查 `/health`、管理员登录和一次小型上游请求。请勿将备份提交到 Git。

## 本地开发与 CI

需要 Rust **1.99**、Node.js **22** 及 npm，SQLite 在 Rust 构建中自带：

```sh
cd web
npm ci
npm run build
cd ..
cargo run --locked
```

本地默认监听 `0.0.0.0:1990`，使用 `DATA_DIR=data`、`WEB_DIR=web/dist`。仅本机访问时设置 `BIND_ADDR=127.0.0.1:1990`。`RUST_LOG` 控制日志级别。

GitHub Actions 包含四项检查：**Rust Quality**（格式和严格 Clippy）、**Backend Tests**（单元测试及模拟上游集成测试）、**Frontend**（全新依赖安装、TypeScript 及生产构建）、**Docker**（Compose 验证、镜像构建、非 root HTTP 冒烟和重启后的初始化持久化）。Docker 在前三项通过后执行，再于默认分支推送时发布镜像。CI 使用临时生成的测试账密，不使用真实上游密钥。测试通过表示已覆盖行为验证完成，不是对所有潜在缺陷的零风险承诺。

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

## 开源协议与第三方声明

前端构建会自动收集锁定依赖的许可声明，包含 Inter 字体的 SIL Open Font License。生成的 `assets/third-party-licenses.txt` 由站点提供，并随镜像保存至 `/app/licenses/frontend-notices.txt`。第三方组件保留各自的许可条款。

本项目以 [MIT 协议](LICENSE) 开源。

### 上游项目与许可遵循

内置指令配置借鉴并改编自 [MDX-Tom/gpt-instruct](https://github.com/MDX-Tom/gpt-instruct)，遵循其 [MIT 许可条款](https://github.com/MDX-Tom/gpt-instruct/blob/main/LICENSE)。感谢上游作者与贡献者的开源工作。

- **上游版权：** Copyright (c) 2026 li lingbo；Copyright (c) 2026 yynxxxxx。
- **许可保留：** 完整版权与许可声明见 [third_party/gpt-instruct.LICENSE](third_party/gpt-instruct.LICENSE)，分发源码或容器时须保留本项目与上游的 MIT 声明。
- **借鉴范围：** 这些指令是导入的配置数据，并非上游应用代码或模型实现。

Claude、Grok、Gemini、DeepSeek 和 GLM 原始 28 项扩展策略参考冷咖啡项目 [3641397194-wq/gpt6-Astra](https://github.com/3641397194-wq/gpt6-Astra)，固定版本为 [09c7799a1fc2ef5f6f6cf4223b7a48abb39d2732](https://github.com/3641397194-wq/gpt6-Astra/tree/09c7799a1fc2ef5f6f6cf4223b7a48abb39d2732)。v0.6.0 保留其中 27 项正文，DeepSeek 4.1 Flash 改用下述寒霜来源。感谢冷咖啡项目作者与贡献者。

- **上游版权与协议：** Copyright (c) 2026 3641397194-wq，MIT License；原始声明保留于 [third_party/coldcoffee.LICENSE](third_party/coldcoffee.LICENSE)，同时分发到容器的 `/app/licenses/coldcoffee.LICENSE`。
- **改编范围：** 将上游 Claude、Grok、Gemini、DeepSeek 和 GLM 客户端的席位指令和 Skill 文本改编为按模型选择的 API 请求内联策略；不包含上游桌面安装器、私有中转工作流或本地 Skill 执行环境。
- **效果边界：** 引用和改编不表示上游认可本项目，也不构成绕过模型拒答、工具可用性或模型能力的保证。分发源码或容器时应保留本项目及上述第三方声明。

DeepSeek 4.1 Flash 的寒霜策略参考 [aimeoa/hanshuang-codex](https://github.com/aimeoa/hanshuang-codex)，固定提交为 [cf9c1537c2bd4ed8577ed6509ee665a39709d7d4](https://github.com/aimeoa/hanshuang-codex/tree/cf9c1537c2bd4ed8577ed6509ee665a39709d7d4)。`memory/README.md` 与 `install-dsh.ps1` 表明 DSH 使用共享 `memory/破甲记忆.md`，没有独立 DeepSeek 正文。本项目仅选取目标绑定、直接执行、真实证据、失败恢复、续接与验证机制，改为 API 实际能力上下文；不导入攻击示例、默认授权假定、安装器或虚构工具环境。正文为 4,748 字节 UTF-8。

- **来源与复现：** [profiles/hanshuang/SOURCE.json](profiles/hanshuang/SOURCE.json) 记录源哈希、选段行号和范围；[third_party/hanshuang](third_party/hanshuang) 保留必要选段和说明。`python scripts/import-hanshuang.py --check` 完全离线核验，去掉 `--check` 可重新生成。
- **许可事实：** 上游 README 声明 MIT，但该提交根目录没有其链接指向的 `LICENSE`；本项目未补造许可证全文或版权所有者，也不将 `seep-kit` 的独立许可视作此提示词许可。见 [third_party/hanshuang/NOTICE.md](third_party/hanshuang/NOTICE.md)，容器附带 `/app/licenses/hanshuang.NOTICE.md`。

Kimi 的四个逐模型策略参考并改编自 [YD-233/kimi-break](https://github.com/YD-233/kimi-break)，固定参考版本为 [`c6a7f6dfffd17c45e60f6eb965f1d4bc169cd8f6`](https://github.com/YD-233/kimi-break/tree/c6a7f6dfffd17c45e60f6eb965f1d4bc169cd8f6)。感谢上游作者与贡献者。

- **上游版权与协议：** Copyright (c) 2026 YD-233，MIT License；完整声明与原始 `README.md`、`SYSTEM.md` 保留于 [third_party/kimi-break](third_party/kimi-break)，容器分发 `/app/licenses/kimi-break.LICENSE`。
- **API 适配范围：** 保留上游人格、行为策略、编码规范与用户风格正文；将 Kimi Code CLI 的模板变量、环境与项目章节改为本次 API 请求实际提供的上下文，不预设本地工具、路径、Skill 或插件。四个模型逐项选择，沿用现有 OpenAI / Anthropic 兼容协议注入链路。单次匹配增加 13,264 字节 UTF-8 指令。
- **复现与边界：** 固定源文件和生成文本的 SHA-256 记录于 [profiles/kimi-break/SOURCE.json](profiles/kimi-break/SOURCE.json)。运行 `python scripts/import-kimi-break.py --check` 可完全离线核验，去掉 `--check` 可重新生成；也可传入该提交的干净上游 checkout。导入器只读取文本数据。模型 ID 为可选配置，不保证中转站提供这些模型；本地测试确认策略注入及原请求保留，实际模型响应效果取决于所用服务。
