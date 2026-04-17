# ContextVM Broken Links Report

> Checked 2026-03-08

## Broken: Domain `contextvm.org` is down (ECONNREFUSED)

All URLs on the `contextvm.org` domain fail with connection refused. The domain appears to have no server running.

| URL | Source | Status |
|-----|--------|--------|
| https://contextvm.org | SDK README, awesome README | ECONNREFUSED |
| https://www.contextvm.org/ | awesome README | ECONNREFUSED |
| https://docs.contextvm.org/ | awesome README, contextvm-docs repo description | ECONNREFUSED |
| https://contextvm.org/blog/ | awesome README | ECONNREFUSED |
| https://docs.contextvm.org/spec/cep-guidelines/ | awesome README | ECONNREFUSED |

**Note:** The GitHub Pages alternatives work fine:
- https://contextvm.github.io/contextvm-site/ (200 OK)
- https://contextvm.github.io/contextvm-docs/ (200 OK)

## Broken: GitHub repo 404

| URL | Source | Status |
|-----|--------|--------|
| https://github.com/ContextVM/keepss-cvm | awesome README | 404 (also tried `keepass-cvm`, `keepasscvm`, `keepass` — all 404) |

## Working links (verified)

- https://contextvm.github.io/contextvm-site/ — 200
- https://contextvm.github.io/contextvm-docs/ — 200
- https://relatr.xyz — 200
- https://github.com/ContextVM/sdk — 200
- https://github.com/ContextVM/ts-sdk — 200 (redirects to sdk)
- https://github.com/humansinstitute/beacon-main — 200
- https://github.com/humansinstitute/craigdavid-cvm — 200
- https://github.com/humansinstitute/retired-in-a-field-cvm — 200
- https://github.com/futurepaul/hypernote-elements — 200
- https://github.com/humansinstitute/nanalytics — 200
- https://github.com/zeSchlausKwab/earthly/tree/master/contextvm — 200
- https://github.com/zeSchlausKwab/wavefunc/tree/main/contextvm — 200
- npm `@contextvm/sdk` — 200
- jsr `@contextvm/gateway-cli` — 200
- signal group link — 200

## Summary

**6 broken links total:**
- 5x `contextvm.org` domain completely down (ECONNREFUSED)
- 1x `keepss-cvm` GitHub repo missing (typo? deleted? private?)
