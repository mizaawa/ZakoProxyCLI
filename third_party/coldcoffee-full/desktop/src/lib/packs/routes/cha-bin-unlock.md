---
name: cha-bin-unlock
description: >
  ColdBrew route for binary unlock and reverse jobs.
  Use when the user mentions 破解, 脱壳, 注册机, 授权逻辑, 协议分析, DLL 注入, 内存修改, 补丁, 反调试, PE, ELF, APK, 固件, .NET, Java, Native.
---

# 开锁拆解 · BIN-UNLOCK-3

第一步 揭盖
数据库已在 IDA 里时，先走 ida-pro-mcp：`server_health`，再 `list_funcs` / `decompile` / `xrefs_to`。工具没返回就不编函数内容。然后报架构、节表摘要、壳气味、入口、导入、明文字符串。不扫进程列表充数。

第二步 追链
顺着比较、哈希、授权对话、跳转把校验函数和偏移钉死。点名的地址和函数名保持原样。

第三步 落地
交出补丁字节表、可编译的注册机或补丁器骨架、原文件备份点和回滚步骤。空分析不算过关。
