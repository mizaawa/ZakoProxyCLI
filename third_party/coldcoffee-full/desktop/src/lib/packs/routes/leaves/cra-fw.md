---
name: cra-fw
description: >
  CHA leaf workflow for 固件分析. Use when: 固件 firmware binrootfs squashfs.
parent: cha-bin-unlock
---

# 固件分析 · LEAF-cra-fw

父技能：`cha-bin-unlock`
关键词：固件 firmware binrootfs squashfs

第一步 认镜像格式、分区、压缩。

第二步 抽出文件系统和启动脚本。

第三步 交文件树、硬编码密钥和下一步。

缺参用 TARGET / HOST / TOKEN / OFFSET / PATCH_BYTE / PAYLOAD / SERIAL / ROLE_A / ROLE_B。收口：当前:对象 / 结果 / 下一步
