# v0.14.5 · 管理安全与维护修复版

v0.14.5 基于已并入 `main` 的 v0.14.4。它保留 51 个模型标识、16 个模板标识和服务商后台余额同步；代理仍按原请求透传。此维护版不携带或注入策略正文。升级前备份 `.env` 和完整 `/data`（包括 `master.key`）。

- 管理员密码（首次初始化和更改）不设字符数上下限，但不能为空；密码仍以 Argon2 哈希保存。系统更新的当前密码复核也接受长密码。
- 安全入口接受有无尾斜杠的同一地址。修改入口后，控制台会直接加载新地址；关闭入口后返回根路径。
- Compose 默认将 `127.0.0.1:1990` 映射到容器的 `1990`。`ZAKO_PORT` 可修改宿主机映射端口，`ZAKO_BIND_IP` 控制宿主机绑定地址；应用在容器内固定监听 `1990`。普通 `PORT` 环境变量不参与端口配置。
- 代理和更新器从同一份检出源码构建。不要用单独下载的 Compose 文件或 `docker compose pull` 替代源码更新。

## 部署

需要 Docker Compose v2+、Git，以及 Python 3.11+（仅一键脚本/版本检查需要 Python）。已有部署必须沿用原项目名、`.env` 及数据卷，不能 `down -v`。

在 `main` 分支的 Git 克隆中更新并部署：

```sh
git switch main
git pull --ff-only
python3 deploy.py
```

如果从 Source code 压缩包解压或检出固定标签，使用 `python3 deploy.py --local`（Windows 可用 `py deploy.py --local`）。首次安装才创建 `.env`，不会覆盖已有配置。

手动部署：仅首次安装且尚无 `.env` 时，先从 `.env.example` 创建它，再运行：

```sh
docker compose -f docker-compose.yaml build --pull
docker compose -f docker-compose.yaml up -d --no-build --pull never --wait
```

核验：`python3 scripts/check-release-version.py --running`。代理和更新器都必须报告 `0.14.5`。源码包不包含本地数据或凭证。
