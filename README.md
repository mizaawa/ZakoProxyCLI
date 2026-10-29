<div align="center">

<img src="web/public/favicon.svg" width="88" height="88" alt="ZakoAI Proxy 标志" />

# ZakoAI Proxy

**多协议接入 · 多账号调度 · 一个清晰好用的管理控制台**

基于 Rust 的自托管 AI API 中转，为你的模型、账号和客户端密钥提供统一管理入口。

[![Release](https://img.shields.io/github/v/release/mizaawa/ZakoProxyCLI?style=flat-square&color=e991ac)](https://github.com/mizaawa/ZakoProxyCLI/releases)
[![CI](https://github.com/mizaawa/ZakoProxyCLI/actions/workflows/ci.yml/badge.svg)](https://github.com/mizaawa/ZakoProxyCLI/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-1.99-dea584?style=flat-square&logo=rust)](Cargo.toml)
[![Docker](https://img.shields.io/badge/Docker-Compose-2496ed?style=flat-square&logo=docker&logoColor=white)](compose.yaml)
[![License](https://img.shields.io/badge/License-MIT-8bbf9f?style=flat-square)](LICENSE)

简体中文 · [English](README.en.md) · [安全说明](SECURITY.md) · [API 契约](API_CONTRACT.md)

**[快速部署](#quick-start) · [配置中转](#configuration) · [客户端接入](#client-api) · [模型与策略](#profiles)**

</div>

---

## 🐟 推荐中转 · 杂鱼 API

> [!TIP]
> ### 让好模型，更好接入。
> **GPT · Claude · Gemini · Grok · 多款国产模型**
>
> 从日常对话到开发调用，为你的 AI 工作流找到合适的模型服务。
> 搭配 **ZakoAI Proxy**，统一管理上游账号、分组与客户端密钥。
>
> ### [🚀 立即访问杂鱼 API →](https://www.zayuapi.com/)
> **[📖 新手部署教程](https://www.mizaawa.com/index.php/archives/95/)** · **[⚡ 开始部署 ZakoAI Proxy](#quick-start)**

| 🤖 丰富模型 | 🔗 服务特色 | 💎 价格定位 |
| :--- | :--- | :--- |
| GPT、Claude、Gemini、Grok 与多款国产模型 | 自有私有原创反代协议号池 | 主打高性价比，按需选择模型服务 |

*以上沿用服务方介绍。模型供应、可用协议、价格与使用规则以杂鱼 API 官网为准。*

---

<a id="overview"></a>

## 为什么选择 ZakoAI Proxy

把上游接入、访问控制与运行状态放在同一个控制台，既能在桌面管理，也能在手机上查看。

| 能力 | 你可以做什么 |
| :--- | :--- |
| **四类请求协议** | 接入 Responses、Responses Compact、Chat Completions 与 Anthropic Messages，保留工具调用和 SSE 流式响应 |
| **账号与模型管理** | 从 API 或服务提供商导入账号，配置单跳模型映射、所属分组及独立 SOCKS5 代理 |
| **分组与客户端密钥** | 用分组与简介组织访问范围，为每把客户端密钥独立选择专用策略和默认策略 |
| **服务提供商** | 连接 Sub2API / New API，查看余额与消费，读取或创建上游密钥并绑定账号 |
| **调度与故障恢复** | 按优先级选择账号，设置重试、RPM、并发和失败冷却，处理匹配账号池的恢复 |
| **统计与调用日志** | 查看 Token、缓存、RPM / TPM、成功率和耗时；分页查看调用记录并导出日志 JSON |
| **自托管与运维** | SQLite 持久化、Docker Compose 部署、控制台版本检查与源码更新 |
| **樱花布丁控制台** | 樱花粉、蜂蜜黄与奶油白主题，配合圆角卡片、图表与「杂鱼酱」插画 |

> [!NOTE]
> 客户端与上游账号使用同一种请求协议。代理不进行 API 协议转换；模型能否调用，以账号实际勾选的模型和支持协议为准。

**阅读导航：** [部署](#quick-start) · [配置](#configuration) · [API](#client-api) · [策略](#profiles) · [运维](#operations) · [开发](#development) · [使用须知与致谢](#notices)

<a id="quick-start"></a>

## 快速开始

### 1. 准备环境

安装 **Docker Engine / Docker Desktop** 与 **Docker Compose v2**，然后获取源码：

```sh
git clone https://github.com/mizaawa/ZakoProxyCLI.git
cd ZakoProxyCLI
cp .env.example .env
```

`.env` 必须先创建：Compose 会把它挂载到容器，控制台也会将 Turnstile 配置写入该文件。升级现有部署时保留已有 `.env`，无需重新复制模板。

<details>
<summary>Linux：允许控制台保存 Turnstile 配置</summary>

将 `.env` 中的 `ZAKO_GID` 设置为该文件所属组的数字 GID，并赋予该组写入权限。可先查看：

```sh
ls -ln .env
chmod 660 .env
```

例如，文件所属组为 `1000` 时，使用 `ZAKO_GID=1000`。调整附加组后重新创建容器，使配置生效。

</details>

### 2. 构建并启动

```sh
docker compose pull
docker compose build --pull
docker compose up -d --no-build
docker compose ps
```

发布镜像为 `ghcr.io/mizaawa/zakoai-proxy-cli:latest`。上述流程先拉取镜像，再从当前源码构建应用与更新器；`build --pull` 同时刷新基础镜像。默认分支 CI 成功后会发布 `latest` 及对应提交 SHA 标签。

### 3. 初始化控制台

打开 **[http://localhost:1990](http://localhost:1990)**，创建管理员账号，密码至少 **12 个字符**。

项目没有默认管理员账号或密码。请先完成初始化，再对外开放服务。初始化完成后，按「分组 → 账号 → 密钥」的顺序配置即可开始接入。

### 4. 验证服务

```sh
curl --fail http://localhost:1990/health
docker compose logs --tail=100 -f
```

<details>
<summary>端口、公网 HTTPS 与流式代理设置</summary>

Compose 默认只监听宿主机 `127.0.0.1:1990`。在项目根目录的 `.env` 中追加或调整：

```dotenv
ZAKO_BIND_IP=127.0.0.1
ZAKO_PORT=1990
TRUST_PROXY=0
```

需要对外开放端口时，将 `ZAKO_BIND_IP` 改为 `0.0.0.0`；`ZAKO_PORT` 控制宿主机端口。

公网部署应通过可信反向代理提供 HTTPS，并让后端端口保持私有。仅在该代理之后设置 `TRUST_PROXY=1`；代理须保留原始 `Host`，并传入 `X-Forwarded-Proto: https`，以启用会话 Cookie 的 `Secure` 属性。

SSE 流式代理需要关闭响应缓冲，读取超时应大于系统设置中的请求超时。

</details>

> [!WARNING]
> `docker compose down` 保留数据卷；`docker compose down -v` 会删除管理员、配置、密钥和统计数据。备份时务必保留完整 `/data`，尤其是 `master.key`。

<a id="configuration"></a>

## 配置中转

### 第一步：创建分组

在 **分组管理** 中建立访问分组，并按需填写支持换行的分组简介。分组负责组织账号与客户端密钥的路由范围；专用策略和默认策略在每把客户端密钥上单独设置。

分组简介默认为空，最多 2,000 个字符，可在密钥选择所属分组时查看。删除分组前，需要先解除账号和密钥对它的引用。

### 第二步：添加上游账号

在 **账号管理 → 添加账号** 中选择“从 API 导入”填写端点与 API key，或选择“从服务商导入”绑定已登录服务提供商的密钥。随后选择支持协议，拉取并勾选实际可用模型，再指定本地所属分组。

| 选项 | 配置说明 |
| :--- | :--- |
| **上游端点** | 可带或不带 `/v1`，按服务商提供的地址填写 |
| **支持协议** | 默认勾选 `responses` / `compact` / `chat` / `anthropic`，按上游实际能力调整 |
| **模型映射** | 将客户端请求名映射到实际模型，目标模型必须已勾选；映射只执行一次 |
| **RPM / 并发** | 设为 `0` 表示不限制 |
| **账号代理** | 可选 SOCKS5 / SOCKS5h，四协议、流式请求和模型获取均沿用该账号代理 |

编辑手填账号时，API key 留空保留原密钥；服务商绑定账号的端点和密钥由服务端读取。解除绑定并改为手填时，必须填写新的 API key。配置代理后，连接失败不会自动改为该账号直连。

### 服务提供商与上游密钥

在侧栏 **服务提供商** 中添加 Sub2API 或 New API 站点，使用站点账号密码或访问令牌登录。卡片显示余额、今日消费、累计消费和累计充值；未返回的数据保持 `—`，估算充值标记“估算”。人民币展示按站点充值倍率换算；登录凭据加密保存，不会在列表中返回。

列表默认置顶 **杂鱼AI · 官方**，固定地址为 `https://www.zayuapi.com`，平台为 Sub2API。官方名称和站点地址不可修改。点击卡片上的 **编辑** 填写站点账号密码或访问令牌，测试登录并保存后即可刷新统计、读取和创建上游密钥、导入账号；配置和加密凭据在重启后保留。未配置凭据时显示“尚未登录，统计暂不可用”。官方预置条目的登录凭据可以编辑，但不能删除该条目；其他服务提供商支持编辑、删除和刷新。

“从服务商导入”先选择站点，再选择 API 密钥。列表展示名称、上游分组、分组倍率和可用状态；上游分组与本地路由分组相互独立。保存后端点和密钥与服务商绑定。再次编辑账号会刷新绑定；确认密钥已删除、停用、过期或额度耗尽后停止参与路由。站点地址或认证身份改变时必须重新选择密钥；暂时的网络错误保留最后确认的绑定。

密钥菜单顶部的 **从服务商创建** 会打开二级弹窗。手动选择实时读取的上游分组后点击“创建”，密钥名称为 `【ZakoAI】` 加原分组名；“取消”不创建。成功后弹窗关闭、列表刷新，并自动选中本次唯一新增且名称、分组匹配的可用密钥，不会误选已有同名密钥。列表延迟更新时可再次刷新；多个新增同名候选需手动选择。失败响应可能发生在上游已保存之后，因此不会自动重试，请先刷新确认，避免重复创建。

Sub2API 使用所选分组创建密钥；New API 默认无限额度、不过期、不启用模型或 IP 限制、不启用跨组重试，需要时可到上游进一步调整。这里创建的是上游密钥，下一步签发的是客户端使用的本地中转密钥。

### 第三步：签发客户端密钥

在 **密钥管理** 中为指定分组创建 API key，并为这把密钥选择专用策略与“开启破甲默认策略”。新密钥默认不选专用策略、默认策略关闭；同组其他密钥不受影响。列表按分组内启用且可用账号的实际模型筛选，失效服务商绑定不提供可选模型；同一实际模型只能勾选一种策略。修改分组会移除不再支持的选择。之后登录仍可复制密钥；修改分组无需更换 key，停用或删除阻止新的请求。

### 第四步：调整系统设置

按需设置安全入口、重试次数、失败阈值、冷却时间、请求超时、统计保留天数和 Turnstile 校验。安全入口将控制台移动到 `/<入口>`，`/health` 与 `/v1/*` 保持不变；保存后请记录新的登录地址。

<details>
<summary>完整配置说明：策略开关、模型别名、代理测试与密码管理</summary>

1. **分组管理：** 创建路由分组并填写可换行简介，不再在分组上选择策略。账号实际模型、支持协议和分组资格独立检查；删除前需解除账号和密钥引用。
2. **账号管理：** 从 API 手填端点及密钥，或选择服务提供商及其可用密钥导入，“支持协议”默认勾选 `responses` / `compact` / `chat` / `anthropic`，可按上游中转站支持情况取消，再拉取并勾选上游实际可用模型、选择所属分组。`chat` 对应 `/v1/chat/completions`，`anthropic` 对应 `/v1/messages`；选择 Anthropic 获取模型时也使用 Anthropic 鉴权。模型名称不限定协议：例如 Grok、Gemini、DeepSeek 或 GLM 可以走中转站提供的 OpenAI 兼容或 Anthropic 兼容接口。可通过 `model_mappings` 设置“请求模型 → 实际模型”，映射目标必须已勾选；映射仅执行一次，不递归串联，未配置映射的请求使用同名模型。端点可带或不带 `/v1`。RPM 和并发限制设为 `0` 表示不限制；手填模式编辑时密钥留空保留原密钥。“模型映射”和“所属分组”之间可选填账号代理，支持 `socks5://host:port`、`socks5h://host:port` 和带用户名密码的链接（特殊字符需百分号编码）。右侧测试按钮通过该代理请求当前上游模型接口，显示耗时、HTTP 状态或脱敏错误；HTTP 401/403 表示代理已连通但上游鉴权失败。代理地址加密保存，编辑留空保留，点击“移除代理”并保存才会删除。保存后四协议、流式请求及模型获取均走该账号代理，代理失败不会自动改为该账号直连；重试使用下一个账号自己的代理。Docker 中 `127.0.0.1` 指容器自身，应填写容器可访问的代理地址。
3. **密钥管理：** 通过 `profile_ids` 与 `default_strategy_enabled` 为每把客户端 API key 独立设置策略。只提供所属分组可用账号的实际模型支持的专用策略；界面取消重叠版本，API 拒绝冲突或不支持的显式选择。命中专用策略时优先使用，否则仅在默认开关开启时兜底，每次最多注入一种正文，不扩大模型范围。新密钥默认 `[]` 与 `false`；停用或删除阻止新请求，API 基础地址为当前站点来源加 `/v1`。
4. **系统设置：** 设置安全入口、最大重试数、失败阈值、冷却时间、请求超时及统计保留天数。修改密码需验证原密码并再次确认新密码，成功后撤销所有会话。此处二次确认不是 TOTP 等独立第二认证因素。

例如，账号配置片段 `{"models":["gpt-6-astra"],"model_mappings":{"chat-main":"gpt-6-astra"}}` 允许客户端请求 `chat-main`，向上游发送的 `model` 为 `gpt-6-astra`；当前客户端密钥勾选 `gpt-6-astra` 策略后，注入该模型的策略，而不是按别名匹配。未设置映射的 `gpt-6-astra` 请求仍使用同名实际模型。所有支持协议使用相同的模型映射与策略匹配规则。

</details>

<a id="client-api"></a>

## 客户端接入

使用控制台生成的**中转密钥**鉴权，可通过 `Authorization: Bearer` 或 `x-api-key` 传入。

| 接口 | 路径 | 用途 |
| :--- | :--- | :--- |
| 模型列表 | `GET /v1/models` | 返回当前分组可用的实际模型和有效请求别名 |
| Responses | `POST /v1/responses` | Responses 请求与 SSE 流式输出 |
| Compact | `POST /v1/responses/compact` | 上下文压缩，需上游支持 |
| Chat Completions | `POST /v1/chat/completions` | OpenAI 兼容消息、工具调用与 SSE |
| Anthropic Messages | `POST /v1/messages` | Anthropic 兼容消息、工具调用与 SSE |

**Chat Completions 客户端**通常使用 `http://localhost:1990/v1` 作为基础地址；**Anthropic 客户端**使用 `http://localhost:1990` 作为服务地址，并选择启用了对应协议的账号。

### 一个请求开始使用

```sh
export ZAKO_API_KEY='YOUR_GENERATED_PROXY_KEY'

curl http://localhost:1990/v1/responses \
  -H "Authorization: Bearer $ZAKO_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"YOUR_REQUEST_MODEL_ID","input":"Hello"}'
```

将 `YOUR_REQUEST_MODEL_ID` 替换为账号已勾选的实际模型名，或配置好的请求别名。流式调用加入 `"stream":true` 并使用 `curl -N`：

```sh
curl -N http://localhost:1990/v1/responses \
  -H "Authorization: Bearer $ZAKO_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"YOUR_REQUEST_MODEL_ID","input":"Hello","stream":true}'
```

### 常见错误速查

| 状态 / 错误码 | 含义 |
| :--- | :--- |
| `429 / rate_limit_exceeded` | 本地账号 RPM 超限 |
| `429 / concurrency_limit_exceeded` | 本地账号并发超限 |
| `503 / upstream_cooldown` | 账号故障冷却，`Retry-After` 提供剩余时间 |
| 上游 HTTP 错误 | 保留真实状态码、脱敏正文及 `Retry-After` |

本地调度错误包含 `x-zako-error-source: local` 和请求 ID；上游错误标记为 `upstream`。完整字段约定见 [API_CONTRACT.md](API_CONTRACT.md)。

<details>
<summary>协议细节：策略注入、工具续接、流结束与错误透传</summary>

提供 `GET /v1/models`、`POST /v1/responses`、`POST /v1/responses/compact`、`POST /v1/chat/completions` 和 `POST /v1/messages`。`/v1/models` 包含当前分组账号已勾选的实际模型及有效映射的请求模型名。鉴权使用控制台生成的中转密钥，通过 `Authorization: Bearer` 或 `x-api-key` 传入。客户端与账号使用同一种请求协议，代理不转换 API 协议或补充 Google 等厂商的原生接口。

Chat Completions 客户端使用当前站点加 `/v1` 作为基础地址，并启用账号的 `chat` 协议。`/v1/chat/completions` 保留原有消息、工具字段和 SSE，按当前客户端密钥的专用优先、默认兜底规则选出的策略作为独立的 `system` 消息前置，保留客户端原有消息及其他请求字段。JSON 和 SSE 响应沿用上游 Chat Completions 格式。

Anthropic 客户端使用当前站点作为服务地址，选择已启用 `anthropic` 协议的账号模型。`/v1/messages` 保留原生消息、工具调用与 SSE，转发 `anthropic-version`（默认 `2023-06-01`）及 `anthropic-beta`。按同一客户端密钥策略选择规则，将选出的正文前置到 `system`，保留原有字符串或文本块数组。模型映射、故障切换、限流及统计沿用相同调度规则；输入 token 包含 Anthropic 缓存读取及写入，缓存命中 token 统计读取量。

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

`YOUR_REQUEST_MODEL_ID` 可以是已勾选的实际模型名或账号配置的请求模型别名。上下文压缩使用 `/v1/responses/compact`，实际支持情况由上游决定。Responses 和 Compact 先解析当前账号的实际模型，再优先选择当前客户端密钥为该模型勾选的专用策略；未匹配时按这把密钥的 `default_strategy_enabled` 决定是否使用默认策略。选中的一种正文前置到 `instructions`，原有客户端指令和其他字段保留；开关关闭且未匹配专用策略时不注入。Chat Completions 与 Anthropic Messages 使用相同规则，分别通过独立 `system` 消息和 `system` 字段注入。SSE 转发同时读取用量和完成状态，上游凭证反射会被脱敏。“破甲”配置属于可选指令文本，不保证抢占上游系统指令优先级、改变模型行为或消除拒答，实际结果取决于上游服务。

工具调用与流结束：Responses 的 `response.completed`、Anthropic 的 `message_stop`、Chat 的 `[DONE]` 或全部 `choices` 的非空 `finish_reason` 都会释放账号并发槽，后续工具结果可立即续接；仍继续透传尾部用量。Chat 中转在完整 `finish_reason` 后正常 EOF、未发送 `[DONE]` 时也视为正常结束；仅部分 choice 结束或没有完成标志仍按截断处理。Responses 因 `max_output_tokens` / `content_filter` 正常停止不会累积账号故障，真正的错误、连接中断和截断仍触发故障冷却。

排查 429：本地 RPM / 并发超限返回 `429`，错误 `code` 分别为 `rate_limit_exceeded` / `concurrency_limit_exceeded`；账号故障冷却返回 `503` 和 `upstream_cooldown`，`Retry-After` 为剩余冷却秒数（RPM 为当前分钟剩余秒数，并发为建议 1 秒）。这些本地调度错误含 `x-zako-error-source: local` 与请求 ID。真实上游 HTTP 错误标为 `upstream`，保留状态、脱敏正文及 `Retry-After`；不会通过更换状态码掩盖上游限流。

</details>

<a id="profiles"></a>

## 模型与策略

当前目录包含 **42 个策略、41 个唯一模型 ID**。专用策略按**映射后的实际模型 ID**精确匹配；GLM-5.3 与 Grok-4.7 各有两个可选版本，同一模型的策略互斥。

| 选择方式 | 生效条件 |
| :--- | :--- |
| **专用策略** | 当前客户端密钥已勾选，且匹配当前账号映射后的实际模型 |
| **默认策略** | 当前密钥未命中专用策略，且自身的 `default_strategy_enabled` 已开启 |
| **不注入策略** | 未命中专用策略，且默认策略开关关闭 |

策略目录描述可选配置，上游是否提供相应模型仍由账号决定。新增轻量策略标记为**未实测**；文本大小、结构检查与本地协议测试不代表真实模型效果。

<details>
<summary>展开完整模型目录</summary>

| 模型系列 | 精确匹配的实际模型 ID |
| --- | --- |
| GPT（保留） | `gpt-5.6-sol`、`gpt-6.1-sol`、`gpt-6-astra` |
| Claude | `claude-fable-5`、`claude-fable-5-1`、`claude-haiku-4-5`、`claude-haiku-5-5`、`claude-opus-4-5`、`claude-opus-4-6`、`claude-opus-4-7`、`claude-opus-4-8`、`claude-opus-5`、`claude-opus-5-5`、`claude-sonnet-4-5`、`claude-sonnet-4-6`、`claude-sonnet-5`、`claude-sonnet-5-5` |
| Grok | `grok-4.7`、`grok-4.6`、`grok-build` |
| Gemini | `gemini-3-flash`、`gemini-3.1-flash-image`、`gemini-3.1-pro`、`gemini-3.1-pro-preview`、`gemini-3.6-flash`、`gemini-3.7-flash`、`gemini-3.8-flash` |
| DeepSeek | `deepseek-v4-flash`、`deepseek-v4-flash-0731`、`deepseek-v4-pro`、`deepseek-v4-pro-0813`、`deepseek-v4.1-flash` / `deepseek-4.1-flash` / `deepseek-flash`（`deepseek-v4.1-flash` 与 `deepseek-4.1-flash` 共用原寒霜策略，`deepseek-flash` 为独立寒霜策略） |
| GLM | `glm-5.3`、`glm-5.3-flash`、`glm-5.3-air` |
| Kimi | `kimi-k3`、`kimi-k2.7`、`kimi-k2.7-code`、`kimi-k2.6` |

</details>

<details>
<summary>策略机制与调度规则：来源、正文体积、重试、冷却与故障转移</summary>

默认策略从现有 `gpt-6-astra` 提示词的任务执行与连续任务机制改编为多模型 API 文本，见 [profiles/default.md](profiles/default.md) 和 [profiles/default.SOURCE.json](profiles/default.SOURCE.json)。正文为 3,844 字节 UTF-8，移除固定模型身份、本机工具环境和强制四文件交付等假设，保留客户端上下文和真实可用工具。开启后，未选专用策略的模型及目录外模型均可使用默认策略，前提仍是账号已勾选该实际模型。界面提示：“开启后，未勾选模型将采取默认提示词注入尝试进行破甲，如造成无法正常使用现象请关闭此选项”。密钥 API 的 `PUT` 省略开关字段保留原值，显式 `false` 关闭；`null` 或非布尔值会被拒绝。

目前 27 个策略继续使用冷咖啡正文，保留上游席位正文、工作链和完整路由技能，单次匹配注入约 64–66 KB 的 UTF-8 文本，会占用上下文并增加输入 token。v0.6.0 将 DeepSeek 4.1 Flash 的一个策略替换为寒霜改编正文，其余 DeepSeek 策略不变。冷咖啡历史来源、逐文件哈希及适配范围记录于 [profiles/coldcoffee/SOURCE.json](profiles/coldcoffee/SOURCE.json)；可用 `python scripts/import-coldcoffee.py UPSTREAM_CHECKOUT` 从指定版本的干净上游副本重新生成，脚本只读取数据，不执行安装器或 Skill。

专用策略按精确模型选择，不按系列通配匹配。寒霜策略沿用 `coldcoffee-deepseek-v4.1-flash-v1`，迁移到密钥的已有选择继续匹配 `deepseek-v4.1-flash` 与 `deepseek-4.1-flash`；新增独立 `hanshuang-deepseek-flash-v1` 策略匹配 `deepseek-flash`，正文相同。名单表示策略选择，并不承诺上游提供这些模型；以账号实际可用模型和协议为准。冷咖啡、寒霜、Kimi 和 AI-Jailbreak 正文均为内联请求文本，代理不会安装本地 Skill、工具或客户端配置，也不会伪造工具执行结果。实际模型行为须在使用的中转站验证。

AI-Jailbreak 六项轻量策略使用 `aijailbreak-<精确模型 ID>-v1`，GLM 与 Grok 的适配正文分别为 2,114 和 2,014 字节 UTF-8，字节数不等于 token 数。它们是基于上游技术交付、简洁输出、不确定性和上下文续接机制编写的英文 API 适配正文，按客户端语言回答；不是原始提示词的直接复制。现有 `coldcoffee-glm-5.3-v1` 与 `coldcoffee-grok-4.7-v1` 保持不变，已有密钥不会自动切换策略版本。新增条目的可选 `metadata` 包含来源 URL、固定提交、`lightweight-api` 版本类型和 `unverified` 验证状态。本地测试确认路由、互斥和原请求保留，尚未进行真实模型效果评测。

本次评估保留其他系列：DeepSeek 候选要求先输出重写请求并固定首轮结构，可能改变客户端的 JSON 或工具工作流；GPT、Claude、Gemini 和 Kimi 候选缺少现有精确型号的效果证据；通用候选也未证明优于现有默认策略。它们不在本版本中替换现有策略，评估范围记录于 [profiles/ai-jailbreak/SOURCE.json](profiles/ai-jailbreak/SOURCE.json)。

调度仅选择匹配当前分组、协议且映射后实际模型已勾选的可用账号，未勾选任何可用模型的账号不接收请求。**优先级数字越小越先尝试**，相同优先级按账号 ID 稳定排序。单请求最多尝试 `最大重试数 + 1` 个不同账号。每次尝试都从原始客户端请求重新应用当前账号的单跳映射，再按实际模型匹配当前客户端密钥的策略；故障转移到其他账号时独立计算，不继承或叠加前一账号的策略。连接失败、HTTP 401、403、408、429 和 HTTP 5xx 可在响应开始前切换到下一账号；只有多个匹配账号时才启用连续失败冷却，单账号池不会冻结。某请求对应的账号池全部进入冷却后，调度会同时清除该池冷却并重新轮询；若再次全部失败，会按相同规则循环。冷却时间到期的账号也会正常恢复。其余 4xx 原样返回，不触发重试。流式响应已开始后中断不会重放请求。RPM 窗口、并发及冷却状态只保存在当前进程，重启后重置；同一个数据卷只运行一个实例。

安全入口将控制台从 `/` 移到 `/<入口>`，`/health` 和 `/v1/*` 不受影响。它是额外的入口隐藏措施，不代替登录认证。保存设置后请记录新的登录地址。

</details>

<a id="operations"></a>

## 运维与数据

### 控制台更新

在 **系统设置 → GitHub 源码更新** 中检查正式 Release，并在验证当前管理员密码后执行更新。更新器会从官方标签构建镜像，展示构建、部署与健康检查日志。

构建期间旧版本继续服务，切换版本时连接会短暂中断。部署或健康检查失败时，恢复旧版本及同一次数据备份。

<details>
<summary>更新器架构、自动检查、备份与升级注意事项</summary>

v0.5.0 起，登录后自动检查新版本，并在页面可见时每 15 分钟复查；后台标签暂停检查，退出登录停止检查。发现更高的正式版本时，页面标题旁显示带圆点的黄色“有新版本”标签，点击进入系统设置并定位更新区域。手动检查与顶部提示同步。查询在服务端合并并发请求，成功缓存 5 分钟，失败退避 1 分钟；未配置更新器也可检查和提示，执行构建更新仍需配置更新器。

v0.3.0 起可在 **系统设置 → GitHub 源码更新** 中检查 GitHub 最新正式 Release。选择更新后需再次输入当前管理员密码；更新器从官方仓库检出对应标签，在后台从源码构建镜像，并显示构建、部署和健康检查日志。构建期间旧版本继续服务，切换版本时会短暂中断连接；页面恢复后可继续查看结果。

Compose 同时启动隔离的 `updater` 服务，不对宿主机发布端口。主程序通过内部 `UPDATER_URL=http://updater:9000/` 及只读 `/control/token` 访问它。Docker socket 仅挂载给更新器，不挂载给公开的代理服务。更新器持有近似宿主机 root 的 Docker 控制能力，请仅在受控服务器上部署，不要对外暴露内部接口或共享令牌。

源码构建成功后，更新器先停止代理并备份完整数据，再启动新镜像；部署或健康检查失败时恢复旧版本及同一次数据备份。更新状态和备份保留在 `update-state` 数据卷中，不能代替异地备份。账号、API key、分组、统计和 `master.key` 随数据卷保留。更新只接受官方仓库的正式版本标签，不支持输入任意地址或执行命令。

旧版部署需先拉取本版本源码并执行一次上述 Compose 构建和启动命令，以安装更新器。使用自定义 Compose 项目名时，保持 `COMPOSE_PROJECT_NAME` 一致；不使用 Compose 或未配置更新器的本地开发环境只展示版本信息，不能在网页执行更新。更新代理镜像不会自行替换更新器或修改宿主机 Compose 文件；发布说明要求调整部署配置时仍需由管理员执行。

</details>

### 日志与统计

仪表盘集中展示用量与趋势；独立的**调用日志**页面提供分页、JSON 导出、状态码、结构化错误、首字耗时和缓存 Token 信息，默认保留最近 **7 天**。

<details>
<summary>统计口径与存储：Token、缓存、UTC、保留期及加密范围</summary>

- 仪表盘展示输入、输出、缓存及未命中 token、缓存命中率、RPM、TPM、总请求、成功率、平均耗时、每日图表、历史区间和 Cyber 标记次数。
- Token 总量、Token 分布及相关缩写显示使用 `K`（千）、`M`（百万）、`B`（十亿）、`T`（万亿），缩写固定两位小数，例如 `90.48M`、`1.00B`；不足一千保持整数。仅更改显示，API 返回的用量仍为原始整数。
- 缓存 token 已包含在输入 token 内，总 token 为输入加输出，不重复计数。用量来自上游 JSON 或完成后的 SSE 用量字段，缺失时计为零，不估算。
- RPM / TPM 为最近 60 秒已结束并记录的请求；每日统计使用 UTC。总量和单密钥用量基于保留中的请求元数据，并非永久计费账本。默认保留 90 天，新请求写入时清理过期记录。
- Cyber 计数表示响应中出现不区分大小写的 `cyber` 文本，每个响应最多计一次。它是文本启发式统计，不等同于特定弹窗、拒答或模型行为的检测结果。
- `/data/zako.db` 保存配置、会话摘要和请求元数据。上游密钥及可再次复制的客户端密钥使用 `/data/master.key` 配合 AES-256-GCM 加密；管理员密码使用 Argon2 摘要。请求日志不持久化提示词或响应正文；SQLite 文件和其他元数据未进行整盘加密。

</details>

### 备份与恢复

请将**整个 `/data`，特别是 `master.key`** 备份到受控存储。停机时可得到一致备份：

```sh
docker compose stop
docker compose cp proxy:/data ./zako-backup
docker compose start
```

恢复时先停止服务，将备份内容放回数据卷 `/data` 根目录并保留 `10001:10001` 所有权。数据库、SQLite 附属文件和 `master.key` 应来自同一次备份；缺少匹配主密钥的数据库不能解密存储的 API key。恢复后检查 `/health`、管理员登录和一次小型上游请求。请勿将备份提交到 Git。

### 版本记录与旧版迁移

当前版本 **v0.13.2** 修复服务商导入状态显示，并兼容新版 New API 系统访问令牌；保留 **v0.13.1** 的聊天布局和 **v0.12.2** 的服务商安全验证与会话续期。界面延续 **v0.10.0 樱花布丁** 主题。

#### v0.13.2 服务商导入与 New API 兼容

- 官方服务商在账号导入列表中按真实状态显示“已连接”“同步失败”“待同步”或“未登录”，避免已可导入时仍显示“未登录”。
- 新版 New API 的 `nap_` 系统访问令牌无需手填用户 ID，按 Bearer 认证读取余额与用量；导入 API 密钥时沿用同一认证。令牌至少需要 `profile:read`、`usage:read`、`api_key:read` 和 `api_key:reveal`，从服务商创建密钥还需 `api_key:write`。
- 兼容新版账号密码登录返回的短期访问令牌，也保留旧版用户 ID、`New-Api-User` 与会话 Cookie 流程。旧版系统令牌仍需用户 ID；旧版令牌在新版 New API 上的有效期由上游控制。无需数据库迁移。

#### v0.13.1 聊天界面优化

- 侧栏分区改为“对话”，菜单名改为“聊天”；聊天内容铺满工作区，背景透明并与上方栏保持统一。
- 复制、编辑和删除操作移到各条消息下方，移除输入区多余提示。
- 整块输入功能板块固定悬浮于聊天内容上方，四周留白、圆角和阴影；桌面与手机均为消息滚动保留足够空间。无需数据库迁移。

#### v0.13.0 游乐场与导航

- 在侧栏顶部新增“聊天 → 游乐场”，并调整密钥管理、服务提供商和调用日志的位置。聊天通过已启用的本地密钥选择分组和模型，复用现有 Chat Completions 转发、流式输出与调用日志；本地密钥与上游密钥均不暴露给浏览器。
- 支持文本、Markdown、CSV、JSON、PDF 文本及 PNG/JPEG/WebP/GIF 图片附件；服务端验证格式、内容与大小。PDF 必须可提取文本。每条消息最多 5 个附件，文本和图片会作为对话内容发往所选上游。
- 可选联网搜索从固定的 Bing RSS 地址取得最多 5 条结果，作为带来源链接的参考上下文发送给模型；若搜索不可用或无结果，会显示错误。搜索内容和附件均按不可信参考资料处理。
- 聊天页铺满内容区，附件、搜索、参数、清空和新对话操作移到输入框下方；分组、模型和密钥选择改为主题内可搜索弹层。对话历史、模型参数和搜索开关保存在当前浏览器。新版本无需数据库迁移。

#### v0.12.2 服务商安全验证与会话续期

- 按 Sub2API 站点公开配置显示 Cloudflare Turnstile，由用户在浏览器完成验证后提交一次性令牌；令牌不写入配置或日志。站点密钥须允许当前控制台域名，真实验证结果由上游判定。
- 测试登录后复用加密会话，保存、刷新、读取分组和导入密钥不重复消耗验证码；访问令牌过期时续期，无法续期且要求验证时提示重新登录。
- 兼容旧版未保存会话令牌的密码账号；未启用 Turnstile 的站点可重新登录。验证码脚本失败支持重试，长时间停留后的保存会重新检查登录。
- 官方“杂鱼AI”名称、`https://www.zayuapi.com` 地址与 Sub2API 平台固定，账号密码或令牌仍可编辑。

#### v0.12.1 官方服务商登录修复

- 官方“杂鱼AI”支持编辑账号密码或令牌、测试登录、保存及刷新，配置在重启后保留。
- 官方名称与 `https://www.zayuapi.com` 地址固定不可修改，前后端同时校验；普通服务商仍可更名或更换地址。
- 未登录时提供“配置登录”入口，避免读取空统计；官方条目保持置顶且不可删除。

#### v0.12.0 服务提供商与密钥级策略

- 新增 Sub2API / New API 卡片、官方杂鱼AI 置顶条目，以及绑定上游密钥的账号导入。
- 支持创建 `【ZakoAI】原分组名` 上游密钥，刷新后自动选中唯一新密钥；结果不确定时先刷新确认，不自动重试。
- 分组增加可换行简介；专用策略与默认策略移到客户端密钥，按分组内可用账号的实际模型筛选，不同密钥互不影响。
- 首次启动将旧分组策略迁移到已有密钥中缺少对应字段的部分；显式 `[]` / `false` 保持不变，旧版准确匹配内置模板的设置继续识别。迁移只执行一次，新密钥仍从空策略和关闭默认策略开始；不激活任意旧自定义指令。
- 服务提供商、加密凭据与账号绑定保留。升级前备份完整 `/data` 与 `master.key`；回退旧版须恢复升级前同一份数据备份。
- 改善手机端圆角控件、弹窗滚动边界，以及侧栏切换时替换内容区域的加载提示。

<details>
<summary>展开 v0.6.0–v0.11.1 更新说明与仓库改名迁移步骤</summary>

#### v0.11.1 Cyber 日志检测与密钥显示修复

修复旧检测扫描整个响应导致的 Cyber 误标记：上游回显的 `instructions`、`input`、`metadata`、`model` 或 `id` 不再参与判断。新检测仅检查助手输出文本和拒绝文本（`refusal`），忽略工具参数、思考内容、错误与元数据，并支持跨流式 delta、跨网络分块识别。

修复适用于升级后的新请求，沿用现有数据库，无需迁移。历史日志未保存响应正文，无法可靠重新判定，因此保留已有标记与统计，不自动清零。

密钥管理统一显示为 `sk-zako-8f2a078••••••••bba6`：保留开头和最后四位，中间使用八个粗圆点遮罩，移除尾部重复遮罩。复制按钮仍复制完整密钥。

#### v0.11.0 GLM / Grok 轻量策略与账号调度

新增六个需要主动勾选的 AI-Jailbreak API 轻量策略，覆盖 `glm-5.3`、`glm-5.3-flash`、`glm-5.3-air`、`grok-4.7`、`grok-4.6` 和 `grok-build`。原 GLM-5.3 / Grok-4.7 冷咖啡策略及已有分组选择保留，同一精确模型的不同策略互斥。目录现为 42 个策略、41 个唯一模型 ID。轻量版标注来源、正文大小与“未实测”状态，不将体积减少或结构分视作效果提升。

仪表盘移除数据导出按钮及对应导出功能，调用日志页的 JSON 导出保留。账号冷却按当前请求的分组、协议和实际模型筛出的候选池处理：只有一个候选账号时不进入冷却，全部候选账号冷却时清除该池冷却后重新轮询；单次请求仍最多尝试 `最大重试数 + 1` 个不同账号，RPM 与并发限制不变。升级保留数据和已有策略选择，无需数据库迁移。

#### v0.10.0 樱花布丁主题

控制台界面整体改为二次元风格的“樱花布丁”主题：以樱花粉、暖蜂蜜黄和奶油白为主色，卡片、按钮、输入框和弹窗采用更大的圆角；侧边栏和顶栏改为浮动卡片，系统设置与调用日志改为卡片布局，图表使用柔和的渐变填充，登录页和空状态新增“杂鱼酱”小鱼插画。界面西文字体由 Inter 改为 Nunito（OFL-1.1，随前端本地打包），中文继续使用系统字体。本版本只修改前端外观，API、数据与部署方式不变，升级无需额外操作。

#### v0.9.3 Turnstile `.env` 配置与 Release 同步

Turnstile 站点密钥和密钥统一保存在项目根目录 `.env`，系统设置保存时会自动更新该文件；旧数据库中的 Turnstile 配置会被清理。系统设置提供后台连接测试和右上角 Toast 提示。GitHub 源码更新以最新正式 Release 为准；当本地版本高于 Release 时，更新按钮会同步到该正式版本。

#### v0.9.0 Turnstile 安全校验与日志统计改进

系统设置支持配置 Cloudflare Turnstile 站点密钥和密钥，登录时启用人机校验；密钥加密保存且不会通过 API 返回。调用日志分页器支持键盘操作和移动端触控，仪表盘对大数量统计使用紧凑显示；新增超过千条 Cyber 事件的统计覆盖测试。

#### v0.8.0 DeepSeek API 兼容策略

为全部五个 DeepSeek 精确模型档案组合一层共享的 API 适配提示，吸收 dsh-purge 的请求期提示组装与上下文连续性思路；策略仍按映射后的实际模型选择，并沿用现有 Responses / Compact、Chat Completions 和 Anthropic 注入链路。原 profile ID 与旧版配置识别规则保留，模型目录数量不变。适配层以当前请求实际携带的消息、工具和结果为准，不假设 Harness 本地文件、hook、审批、沙箱、跨请求记忆或权限，也不声称修改上游模型策略。来源版本、改编范围和 MIT 声明见 [profiles/dsh-purge/SOURCE.json](profiles/dsh-purge/SOURCE.json) 与 [third_party/dsh-purge/NOTICE.md](third_party/dsh-purge/NOTICE.md)。

#### v0.7.0 调用日志与请求链路修复

密钥管理下新增独立调用日志，默认保留最近 7 天，每页 20 条，支持分页跳页、JSON 导出、清空日志、状态码和结构化 JSON/SSE 错误详情，并记录首字耗时及缓存读写 Token；仪表盘不再重复展示调用日志。Responses / Chat 流式工具请求现在在确认终止事件后及时释放并发槽；正常的长度限制或内容过滤不会误判为账号故障。账号冷却返回带真实剩余时间的 503，RPM 和并发本地限制保留 429，并提供机器可读错误原因。真实上游的 HTTP 状态、脱敏错误正文及 Retry-After 继续透传。

#### v0.6.0 仓库改名后的首次升级

官方仓库已改名为 ZakoProxyCLI。v0.5.0 及更早版本的更新器严格匹配旧 Release URL，因此仓库改名后网页检查或更新可能失败。请在原部署目录执行一次以下操作，同时替换 proxy 和 updater；保持原目录、Compose 项目名和数据卷不变。若本地修改导致拉取失败，请先处理冲突。

```sh
git remote set-url origin https://github.com/mizaawa/ZakoProxyCLI.git
git pull --ff-only origin main
docker compose build --pull
docker compose up -d --no-build
docker compose ps
```

镜像仍发布至 ghcr.io/mizaawa/zakoai-proxy-cli，保持现有部署兼容。v0.6.0 识别新旧官方 Release URL；本次首次迁移不能依赖旧更新器自动完成。

</details>

<a id="development"></a>

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

<a id="notices"></a>

## 使用须知与开源致谢

### 使用须知

> [!WARNING]
> **严禁将本项目用于任何违法违纪活动。**

本项目仅供合法、合规的学习、研究和在你本人有权操作的环境里使用。包括但不限于：未经授权的入侵、攻击、渗透、窃取数据；诈骗、洗钱、传播恶意程序；生成或传播违法有害内容；侵犯他人隐私、知识产权和其他合法权益；违反所在地法律法规或第三方平台、模型服务的使用条款。

使用者须自行遵守所在国家和地区的法律法规，以及相关软件和服务的使用条款。使用本项目产生的一切行为和后果，由使用者自行承担，与项目作者及贡献者无关。如果你不同意以上内容，请不要下载、安装或使用本项目。

<details>
<summary>English disclaimer</summary>

> [!WARNING]
> **This project must NOT be used for any illegal or unlawful activity.**

It is provided only for lawful and compliant learning, research, and use in environments you are authorized to operate. Prohibited uses include, without limitation: unauthorized intrusion, attacks, penetration, or data theft; fraud, money laundering, or distributing malware; generating or spreading illegal or harmful content; infringing others' privacy, intellectual property, or other legal rights; and violating applicable laws or the terms of use of third-party platforms and model services.

You are solely responsible for complying with the laws and regulations of your jurisdiction and with the terms of the software and services you use. All actions and consequences arising from the use of this project are borne entirely by the user, and the authors and contributors accept no liability. If you do not agree, do not download, install, or use this project.

</details>

### 开源协议

前端构建会自动收集锁定依赖的许可声明，包含 Nunito 字体的 SIL Open Font License。生成的 `assets/third-party-licenses.txt` 由站点提供，并随镜像保存至 `/app/licenses/frontend-notices.txt`。第三方组件保留各自的许可条款。

本项目以 [MIT 协议](LICENSE) 开源。

感谢 **gpt-instruct、冷咖啡、寒霜、kimi-break 与 AI-Jailbreak** 等上游项目的开源工作。来源版本、改编范围、许可保留与复现方式详见下方说明。

<details>
<summary>展开完整第三方来源、许可声明与复现方式</summary>

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

GLM / Grok 的六个轻量候选参考 [Baimuking/AI-Jailbreak](https://github.com/Baimuking/AI-Jailbreak)，固定提交为 [`8c278b10f47f305546a52d7a0ce044d2c643dc71`](https://github.com/Baimuking/AI-Jailbreak/tree/8c278b10f47f305546a52d7a0ce044d2c643dc71)。

- **上游版权与协议：** Copyright (c) 2026 Baimuking，MIT License。原始参考正文、许可和改编说明保留于 [third_party/ai-jailbreak](third_party/ai-jailbreak)，容器附带 `/app/licenses/ai-jailbreak.LICENSE` 与 `/app/licenses/ai-jailbreak.NOTICE.md`。
- **API 适配范围：** 保留直接交付、完整产物、明确不确定性及上下文续接思路，改为实际请求上下文和工具能力；去掉固定本地身份、默认全部授权、指令优先级覆盖、无条件禁用措辞和跨请求记忆假设。未执行或安装上游 CLI、安装器或代理配置。
- **来源与复现：** [profiles/ai-jailbreak/SOURCE.json](profiles/ai-jailbreak/SOURCE.json) 记录源文件与产物 SHA-256、字节数和评估结论。`python scripts/import-ai-jailbreak.py --check` 完全离线核验，去掉 `--check` 可重新生成。上游卡片和本站适配均为未实测状态，结构分及协议测试不代表模型效果或破甲成功率。

</details>

---

<div align="center">

**ZakoAI Proxy · 自托管，让你的 AI 接入更有条理。**

[回到顶部](#zakoai-proxy) · [查看 Releases](https://github.com/mizaawa/ZakoProxyCLI/releases) · [提交 Issue](https://github.com/mizaawa/ZakoProxyCLI/issues)

</div>
