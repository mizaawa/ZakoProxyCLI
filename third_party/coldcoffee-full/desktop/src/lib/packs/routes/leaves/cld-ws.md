---
name: cld-ws
description: >
  CHA leaf workflow for WebSocket. Use when: WebSocket 握手 订阅 推送.
parent: cha-apicloud
---

# WebSocket · LEAF-cld-ws

父技能：`cha-apicloud`
关键词：WebSocket 握手 订阅 推送

第一步 抓握手头和首帧。

第二步 试订阅越权、重放。

第三步 交帧模板和问题。

缺参用 TARGET / HOST / TOKEN / OFFSET / PATCH_BYTE / PAYLOAD / SERIAL / ROLE_A / ROLE_B。收口：当前:对象 / 结果 / 下一步
