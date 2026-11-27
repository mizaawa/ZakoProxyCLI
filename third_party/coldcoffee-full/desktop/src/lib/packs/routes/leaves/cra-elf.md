---
name: cra-elf
description: >
  CHA leaf workflow for ELF 分析. Use when: ELF so 动态链接 GOT PLT.
parent: cha-bin-unlock
---

# ELF 分析 · LEAF-cra-elf

父技能：`cha-bin-unlock`
关键词：ELF so 动态链接 GOT PLT

第一步 读 ELF 头、段、动态符号。

第二步 追 GOT/PLT 和 init 数组。

第三步 交符号地图和挂钩点。

缺参用 TARGET / HOST / TOKEN / OFFSET / PATCH_BYTE / PAYLOAD / SERIAL / ROLE_A / ROLE_B。收口：当前:对象 / 结果 / 下一步
