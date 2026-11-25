# Source Code Escrow Agreement Terms

## Deposit Verification & Cadence

Licensor shall deposit the complete buildable source tree, dependency lockfiles, and reproducible build instructions with the Escrow Agent within 14 days of each major release and at least once every quarter. Level-2 verification requires the Escrow Agent to compile the deposited source inside a clean isolated container and record the SHA-256 digest of the resulting binary.

## Release Conditions

The Escrow Agent shall release the deposited materials to Licensee only upon documented occurrence of a release event: Licensor enters involuntary bankruptcy proceedings not dismissed within 60 days, permanently discontinues commercial support for the licensed software without offering a compatible successor, or fails to cure a critical severity-1 defect for 45 consecutive days after written default notice.
