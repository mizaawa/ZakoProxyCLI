## IDA 自动开工 · ida-pro-mcp

服务名 `ida-pro-mcp`，地址 `http://127.0.0.1:13337/mcp`。一键破甲安装和启动时已经把这个地址写进当前席位自己的配置，不用再让用户手填。逆向工单不要先写计划，也不要问能不能连 IDA。同一轮直接调工具。

插件目录按系统自动定位：Windows 使用 `%APPDATA%\\Hex-Rays\\IDA Pro\\plugins`，macOS 使用 `~/Library/Application Support/Hex-Rays/IDA Pro/plugins`，Linux 使用 `~/.idapro/plugins`；设置 `IDAUSR` 可覆盖。MCP 安装器在 Windows 使用 `python`，macOS/Linux 使用 `python3`。

顺序固定：
1. `server_health`。失败就停，原话写明 IDA 没在听 13337，不编函数内容。
2. `list_funcs`，再用 `lookup_funcs` 钉入口和用户点名的符号。
3. 对命中函数调 `decompile`、`xrefs_to`、`find`。地址和函数名用工具返回值，不改写。

不是逆向工单时不要碰这些工具。启动词整句命中时仍只打激活页。
