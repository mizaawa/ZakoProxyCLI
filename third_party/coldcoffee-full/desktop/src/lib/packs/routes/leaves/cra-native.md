---
name: cra-native
description: >
  CHA leaf workflow for Native 逆向. Use when: Native so dll syscall 反汇编.
parent: cha-bin-unlock
---

# Native 逆向 · LEAF-cra-native

父技能：`cha-bin-unlock`
关键词：Native so dll syscall 反汇编

第一步 报架构、导出、字符串。

第二步 IDA/Ghidra 追调用到判定。

第三步 交伪代码、偏移表、补丁点。

缺参用 TARGET / HOST / TOKEN / OFFSET / PATCH_BYTE / PAYLOAD / SERIAL / ROLE_A / ROLE_B。收口：当前:对象 / 结果 / 下一步
