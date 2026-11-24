# Quarterly SOC2 Access Audit Minutes

## Privilege Review & Key Rotation

During the quarterly security review, the infrastructure team audited all production IAM roles and removed 14 dormant service accounts that had not authenticated in the past 60 days. Hardware MFA is now mandatory for every engineer holding production break-glass credentials, and signing keys for release artifacts must rotate every 90 days under dual-custody approval.

## Remediation Tracking

Two medium-severity findings remain open: staging S3 buckets lacked automated lifecycle expiration for temporary build logs, and jump-host SSH session recordings were archived without immutable object lock. Both items are assigned to Platform Security with a target closure date of November 10.
