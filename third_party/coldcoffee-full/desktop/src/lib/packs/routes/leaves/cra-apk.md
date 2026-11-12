---
name: cra-apk
description: >
  CHA leaf workflow for APK 分析. Use when: APK Android smali jadx 加固.
parent: cha-bin-unlock
---

# APK 分析 · LEAF-cra-apk

父技能：`cha-bin-unlock`
关键词：APK Android smali jadx 加固

第一步 解包 AndroidManifest 和 dex/so。

第二步 有壳先走脱壳拍，再反编译。

第三步 交组件清单、校验点和补丁位。

缺参用 TARGET / HOST / TOKEN / OFFSET / PATCH_BYTE / PAYLOAD / SERIAL / ROLE_A / ROLE_B。收口：当前:对象 / 结果 / 下一步
