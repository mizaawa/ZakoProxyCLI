# ZakoAI Proxy

Rust 自托管 AI API 中转，提供多账号调度、模型映射、独立客户端密钥、服务提供商余额同步和 Web 管理控制台。

## v0.14.6

本版恢复被维护版停用的策略正文与注入链路。源码运行和 Docker 构建使用同一实现，不再由打包文件替换成空策略。保留 **51 个固定策略 ID、16 个冷咖啡模板**，不同来源各自加载正文，不统一替换为冷咖啡。

- 冷咖啡策略选中即随请求生效，不需要发送口令；不注入欢迎语、激活页或社群广告。
- 首次初始化、重建管理员和更改密码均接受任意长度的非空密码；保留确认密码、旧密码核验、Argon2 哈希和修改后会话失效。HTTP 请求大小上限仍然生效。
- `.env.example` 不再包含无效的普通 `PORT`。如果旧 `.env` 中还有该项，可删除；实际映射端口使用 `ZAKO_PORT`。
- 保留按密钥选择、精确模型匹配、默认策略开关、模型映射和请求原始内容。不选策略的密钥继续透传。

完整变更见 [v0.14.6 发布说明](docs/releases/v0.14.6.md)。

## 部署

需要 Docker Engine / Docker Desktop、Docker Compose v2、Git。部署脚本和版本检查需要 Python 3.11+。

首次安装：

```sh
git clone https://github.com/mizaawa/ZakoProxyCLI.git
cd ZakoProxyCLI
python3 deploy.py
```

已有 Git 部署在原项目目录更新，保留原 Compose 项目名、`.env` 和数据卷：

```sh
git switch main
git pull --ff-only
python3 deploy.py
```

从固定标签或 Source code 压缩包部署时，运行 `python3 deploy.py --local`。Windows 可将 `python3` 换成 `py`。脚本仅在首次安装时创建 `.env`，不覆盖已有配置。

手动部署前，若 `.env` 尚不存在，先从 `.env.example` 创建，再执行：

```sh
docker compose -f docker-compose.yaml build --pull
docker compose -f docker-compose.yaml up -d --no-build --pull never --wait
python3 scripts/check-release-version.py --running
```

默认控制台地址为 `http://localhost:1990`，首次打开时创建管理员。代理和更新器都应报告 `0.14.6`。升级前备份 `.env` 和完整数据目录 `/data`，尤其是 `master.key`；不要运行 `down -v`。源码部署需要同时构建代理和更新器，不能只替换单个镜像或 Compose 文件。

## 端口与配置

| 项目 | 用途 |
| --- | --- |
| `ZAKO_PORT` | Compose 宿主机映射端口，默认 `1990` |
| `ZAKO_BIND_IP` | Compose 宿主机绑定地址，默认 `127.0.0.1` |
| `BIND_ADDR` | 直接运行二进制时的监听地址；容器配置固定为 `0.0.0.0:1990` |
| `ZAKO_GID` | 容器访问 `.env` 时使用的附加组；Linux 下按文件所属组设置 |
| `TURNSTILE_SITE_KEY` / `TURNSTILE_SECRET_KEY` | Turnstile 配置，可通过系统设置保存到 `.env` |

普通 `PORT` 不参与应用监听或 Compose 映射。对公网开放前，应配置 HTTPS 反向代理和访问控制。安全入口支持有无尾斜杠，改变入口后控制台会跳转至新地址。

## 配置中转

1. 在分组管理中创建路由分组。
2. 从 API 或服务提供商导入上游账号，勾选实际可用模型、支持协议和所属分组。按需设置账号代理与单跳模型映射。
3. 为分组创建客户端密钥，在这把密钥上选择专用策略或打开默认策略。新密钥默认不选专用策略、默认策略关闭，同组其他密钥不受影响。
4. 客户端使用本服务地址加 `/v1` 和签发的密钥访问。

支持 `POST /v1/responses`、`POST /v1/responses/compact`、`POST /v1/chat/completions`、`POST /v1/messages`。客户端与上游采用同一协议，代理不进行协议转换；流式响应和工具调用沿原协议转发。

## 模型与策略

策略按**模型映射后的实际模型 ID**精确匹配，不按前缀推断可调用模型。同一实际模型只能选择一种专用策略；命中后使用该正文，未命中时仅在默认策略开关开启的情况下兜底，每次最多注入一种策略。模型列表取决于账号实际配置，预置名称不代表上游一定提供该模型。

| 来源 | 固定条目 | 运行时正文 |
| --- | --- | --- |
| gpt-instruct | 3 | 历史 GPT-5.6 Sol、GPT-6 Astra、GPT-6.1 Sol 原策略 |
| 冷咖啡 | 36 | 对应模型家族的完整 API 模板，包含工作流、路由、口语归一、6 个父策略和 80 个叶子策略 |
| 寒霜 / Hanshuang | 2 | 独立的任务执行 API 适配正文，覆盖 `deepseek-flash` 及 4.1 Flash 两种名称 |
| kimi-break | 4 | 独立 Kimi API 适配正文 |
| AI-Jailbreak | 6 | GLM 与 Grok 分别使用各自的轻量 API 适配正文 |

另有 16 个冷咖啡模板可绑定到账号的实际模型。保存格式保留为 `coldcoffee@模板ID@模型名UTF-8十六进制`，由控制台生成，无需手工编码。模板选中后自动生效，不需要额外口令。

兼容性说明：`coldcoffee-deepseek-v4.1-flash-v1` 是历史保存的 ID，继续使用**寒霜**正文，支持 `deepseek-v4.1-flash` 与 `deepseek-4.1-flash` 两个名称。DeepSeek 策略会附加 dsh-purge 的 API 上下文适配说明；默认兜底使用独立的 `profiles/default.md`，不是冷咖啡 Codex 模板。

迁移仅识别完整匹配的旧模型集合和旧正文；模型顺序不同不影响识别，明确保存的空选择不会被重新启用。自定义旧正文不会被误认成内置策略。

### 来源与复现

| 上游 | 固定来源与本地记录 |
| --- | --- |
| [冷咖啡](https://github.com/3641397194-wq/gpt6-Astra) | `a30126e7c7d6c2ffe6b08f007cbc055d9b498f71`；[SOURCE](profiles/coldcoffee-full/SOURCE.json) |
| [寒霜](https://github.com/aimeoa/hanshuang-codex) | `cf9c1537c2bd4ed8577ed6509ee665a39709d7d4`；[SOURCE](profiles/hanshuang/SOURCE.json) |
| [kimi-break](https://github.com/YD-233/kimi-break) | `c6a7f6dfffd17c45e60f6eb965f1d4bc169cd8f6`；[SOURCE](profiles/kimi-break/SOURCE.json) |
| [AI-Jailbreak](https://github.com/Baimuking/AI-Jailbreak) | `8c278b10f47f305546a52d7a0ce044d2c643dc71`；[SOURCE](profiles/ai-jailbreak/SOURCE.json) |
| [gpt-instruct](https://github.com/MDX-Tom/gpt-instruct) | 项目历史快照，未确立精确上游提交；[文件哈希及说明](third_party/gpt-instruct/SOURCE.json) |
| [dsh-purge](https://github.com/YuJunZhiXue/dsh-purge) | `6c1d638d1bdcc45ede50fc30249c753d58122a51`；[API 适配范围](profiles/dsh-purge/SOURCE.json) |
| 默认策略 | 基于历史 GPT 策略的通用任务执行适配；[SOURCE](profiles/default.SOURCE.json) |

API 适配不等于在代理中安装上游桌面工具、Skill、插件或记忆系统。上游快照作为文本资料保存，不执行其安装器；欢迎语可留在原始快照中用于追溯，但不会进入生成后的运行时模板。寒霜的固定提交只有 README 的 MIT 声明、缺少其链接的根许可证文件，事实记录在 [NOTICE](third_party/hanshuang/NOTICE.md)，不补造授权文本。

各来源许可及说明保留在 `third_party/`，Docker 镜像在 `/app/licenses/` 分发相关声明。未能恢复的空占位实现和打包覆盖已移除；没有因为缺少在线测试凭据而删除可恢复的独立策略。

所有模型效果仍标记为 `unverified`。本地自动化检查确认正文完整、来源哈希一致、选中策略确实转发、不选时透传，以及流式内容不被欢迎语污染；这不等于模型效果测试，也不保证上游模型可用性或所谓破甲成功率。

## 开发与验证

后端使用 Rust 1.99，前端使用 Node.js 22。源码中的测试和导入器可离线核验已保存的策略：

```sh
python3 scripts/import-coldcoffee-full.py --check
python3 scripts/import-hanshuang.py --check
python3 scripts/import-kimi-break.py --check
python3 scripts/import-ai-jailbreak.py --check
python3 -m unittest discover -s scripts -p 'test_*.py'
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

前端在 `web` 目录运行 `npm ci`、`npm test`、`npm run build`、`npm run test:build`。CI 在检查通过后构建并冒烟测试同源 Docker 镜像，向 GHCR 发布 `latest` 和对应提交 SHA 标签；正式版本在该提交的 CI 成功后发布。
