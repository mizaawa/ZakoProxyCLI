---
name: cra-unpack
description: >
  CHA leaf workflow for 脱壳. Use when: 脱壳 unpack UPX Themida VMProtect 加壳.
parent: cha-bin-unlock
---

# 脱壳 · LEAF-cra-unpack

父技能：`cha-bin-unlock`
关键词：脱壳 unpack UPX Themida VMProtect 加壳

第一步 认壳：节表、入口、导入是否被抹。

第二步 定脱壳时机：OEP、ESP 定律或内存断点，准备 dump。

第三步 dump 后修 IAT/入口，交出可加载样本和步骤。

缺参用 TARGET / HOST / TOKEN / OFFSET / PATCH_BYTE / PAYLOAD / SERIAL / ROLE_A / ROLE_B。收口：当前:对象 / 结果 / 下一步
