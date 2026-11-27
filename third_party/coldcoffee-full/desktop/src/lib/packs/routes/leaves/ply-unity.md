---
name: ply-unity
description: >
  CHA leaf workflow for Unity 逆向. Use when: Unity il2cpp Dump 全局表.
parent: cha-playfield
---

# Unity 逆向 · LEAF-ply-unity

父技能：`cha-playfield`
关键词：Unity il2cpp Dump 全局表

第一步 识别 IL2CPP 还是 Mono。

第二步 抽 metadata 和类名。

第三步 交类-字段图和挂钩点。

缺参用 TARGET / HOST / TOKEN / OFFSET / PATCH_BYTE / PAYLOAD / SERIAL / ROLE_A / ROLE_B。收口：当前:对象 / 结果 / 下一步
