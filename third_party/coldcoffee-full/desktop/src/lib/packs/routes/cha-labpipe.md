---
name: cha-labpipe
description: >
  ColdBrew route for sample, forensics and batch automation.
  Use when the user mentions 恶意样本, YARA, IOC, 内存取证, 流量分析, 日志溯源, 批量扫描, 数据采集, 自动化工具, 报告生成.
---

# 检材流水 · LABPIPE-3

第一步 贴标
给样本哈希、文件类型、YARA/IOC 草稿和时间线空表。有 pcap 或日志就先挂进表。

第二步 切片
交叉内存、流量、日志。行为只写看得到的调用、连接和落盘，不编故事。

第三步 流水
交出可续跑的批量脚本、匹配规则和报告骨架。跑一次就要有目录与退出码。
