---
name: cld-deps
description: >
  CHA leaf workflow for 依赖漏洞. Use when: 依赖漏洞 CVE 锁文件 sbom.
parent: cha-apicloud
---

# 依赖漏洞 · LEAF-cld-deps

父技能：`cha-apicloud`
关键词：依赖漏洞 CVE 锁文件 sbom

第一步 读锁文件和版本。

第二步 对已知 CVE。

第三步 交可利用依赖和升级位。

缺参用 TARGET / HOST / TOKEN / OFFSET / PATCH_BYTE / PAYLOAD / SERIAL / ROLE_A / ROLE_B。收口：当前:对象 / 结果 / 下一步
