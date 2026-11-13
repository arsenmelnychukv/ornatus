# 0006: License and contributions

Status: accepted, 2026-11-12

## Context

The repository is public. The project is a solo effort and should stay that way.

## Decision

- **License.** Dual MIT or Apache-2.0, the Rust convention. Anyone may read, fork and modify the code under those terms.
- **No outside contributions.** The maintainer does not accept pull requests, issues or patches from others. The repository is for viewing and forking only.
- **Enforcement at repo creation.** Turn off Issues and Discussions, state the policy in the README, protect `main`, and check which GitHub controls exist for limiting pull requests on a public repo (GitHub cannot switch pull requests off entirely).
- **Commit identity.** Commits use a repo-local name and the GitHub noreply email. No co-author trailers.

## Consequences

- No contribution guide, no code of conduct needed for now.
- Forks are allowed, so the license files (`LICENSE-MIT`, `LICENSE-APACHE`) must exist before the first public release.
