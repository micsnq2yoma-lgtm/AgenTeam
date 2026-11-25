# On-Premise Deployment Hardware Sizing Guide

## Recommended Server Profiles

| Deployment Tier | Active Users | CPU Cores | Memory (RAM) | NVMe SSD Storage |
| --- | --- | --- | --- | --- |
| Small Team | Up to 50 | 4 vCPU | 16 GB | 200 GB |
| Mid-Size Org | 51 – 300 | 8 vCPU | 32 GB | 1 TB |
| Enterprise Cluster | 301 – 2,000 | 16 vCPU x 3 nodes | 64 GB / node | 4 TB RAID-10 |

## Disk Throughput & Swap Guidelines

Full-text indexing and vector search require sustained random IOPS above 3,000; spinning HDD volumes are unsupported in production. Disable Linux swap on database and search nodes, or set `vm.swappiness=1` so memory-mapped index segments are never paged out unexpectedly.
