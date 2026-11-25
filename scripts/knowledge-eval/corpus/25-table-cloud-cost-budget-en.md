# Q4 Cloud Infrastructure Cost Allocation

## Monthly Budget by Service Domain

| Service Domain | Compute (USD) | Managed DB (USD) | Object Storage & CDN (USD) | Monthly Cap (USD) |
| --- | --- | --- | --- | --- |
| Core API & Sidecar Fleet | 18,500 | 9,200 | 1,800 | 32,000 |
| Search & Indexing Cluster | 7,400 | 0 | 2,600 | 11,500 |
| Observability & Telemetry | 4,100 | 2,300 | 3,900 | 11,000 |
| CI & Ephemeral Preview Envs | 5,200 | 600 | 900 | 7,500 |

## Cost Anomaly Alerts

Automated billing monitors trigger a Slack alert whenever any service domain exceeds 80% of its monthly cap before day 20, or when daily spend spikes more than 35% above the trailing 7-day moving average. Reserved instance purchases require Finance sign-off for commitments exceeding 12 months.
