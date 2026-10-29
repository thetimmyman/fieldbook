# Synthetic dependency catalog
## Shipped runtime crates
### Pins
| Crate | Version (released) | Features | Used by | Licence | MSRV |
|---|---|---|---|---|---|
| `sample-crate` (optional) | | default | core | MIT | 1.85 |

## Test-only and dev-only crates
| Crate | Version (released) | Features | Where | Licence | MSRV | Why | Maintainer health |
|---|---|---|---|---|---|---|---|
| `sample-test` | 2.0.0 | default | dev-dependency | MIT | 1.85 | Synthetic fixture | Synthetic maintainer |

## Developer tools
| Tool | Version (released) | Joins | Note |
|---|---|---|---|
| sample tool (optional) | 9.0.0 | Now | Synthetic pin |

## CI actions
| Action | Version (released) | Commit SHA | Note |
|---|---|---|---|
| `example/action` | v1.0.0 | `bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb` | Deliberate mismatch |
