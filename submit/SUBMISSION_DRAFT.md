# lmsr-sol: submission draft (Solana Perps & Prediction Markets hackathon)

Status: **DRAFT, NOT SUBMITTED.** Nothing here has been entered on the site.
Prepared 21 Nov 2026 (UK time). Listing and form checked 21 Nov 2026 from the public pages
`hackathons.solana.com/hackathons/perps-and-prediction-markets` (+ `/how-it-works`, `/submit` JS bundle).

Legend: **[BLANK]** = must stay empty until the stated condition is met.

---

## 0. Listing facts (checked against the live listing, 21 Nov 2026)

| Claim from MMG | Live listing | Verified? |
|---|---|---|
| 754 registered | `registrationCount: 754` | Yes |
| 0 submissions | `submissionCount: 0` | Yes |
| Deadline 5 Dec 21:00 UK | `submission_deadline: 2026-12-05T21:00:00Z` = **Sat 5 Dec 2026, 21:00 UK (GMT)** | Yes |
| Public repo + short demo required | Hackathon `rules: null`, `status: scheduled`, `launch_date: null`. "Public repo + short demo" wording appears only in the **PREDICT pm-AMM bounty** eligibility (bounty skipped). `/how-it-works` lists all submission items as **optional**: GitHub repo, live demo (devnet or mainnet), pitch video (max 3 min), technical video (5 min walkthrough), team members, sponsor tracks (up to 3). Form says: "Add at least one link judges can use to review your work." | **Partly**: not a stated main-track rule today; recheck when rules publish / at launch |

Other facts: main track $100k (1st $50k / 2nd $30k / 3rd $20k), judged by the Solana Foundation across all
submissions; projects stay private until after judging; main-track winners are contacted by the Solana Foundation.

---

## 1. Form step "Project Info"

**Project Name**
> lmsr-sol

**Short Description** (form max 280 chars; this is 229)
> An N-outcome LMSR prediction-market maker as a native Solana program. Exact fixed-point math with maker-protective rounding: a buy-then-sell round trip can never profit, and the subsidy always covers every payout. Live on devnet.

**Full Description (Markdown)** (form max 5000 chars; this draft is 2,579)

```markdown
## What it is
lmsr-sol is a logarithmic market scoring rule (LMSR) automated market maker for prediction markets, written as a native Solana program (no Anchor). A creator opens a market with 2 to 8 outcomes and any SPL token as collateral; traders buy and sell outcome shares against the curve at any time; the creator resolves a winner; winning shares redeem 1:1 for collateral.

LMSR always quotes a price, so thin markets never stall, and its prices are probabilities that sum to 1.

## Why the math is the product
An on-chain market maker that rounds the wrong way can be drained one dust unit at a time. lmsr-sol is built around maker-protective rounding:

- **Buys** pay C(q') - C(q) plus an explicit error budget, rounded **up** to token base units.
- **Sells** pay C(q) - C(q') minus the error budget, rounded **down** (floored at 0).
- **Prices** are rounded down, so they sum to at most 1.
- **Subsidy**: the creator locks b·ln(n) plus the budget, so the vault always covers the largest outstanding outcome.

Math is WAD (1e18) fixed point in u128 with a 256-bit mul_div; exp only ever sees x <= 0 and ln only x >= 1, so nothing overflows.

## Proof on devnet
Program `7uKjZAG4m5vFd77rWD1kasKV2YwQrzSaHeeFM6a7NWsz` (84,320 bytes, BPF upgradeable loader).

Smoke test (3-outcome market, b = 100 tokens, 6-decimal test SPL mint):
- create_market locked a subsidy of 109,861,229 base units.
- buy 40 shares (40,000,000 base units) of outcome 0: paid **15,181,215**.
- sell the same 40 shares: returned **15,181,214**. The market kept **1** base unit: a round trip can only lose.
- buy 25 shares of outcome 2 for 9,045,765; resolve outcome 2; redeem paid exactly **25,000,000**.

Every amount is also printed in the program logs, visible on Solana Explorer.

## Tests
- 18/18 tests pass (`cargo test --release --workspace`).
- 306 vectors against an 80-digit exact reference: buy >= exact cost, sell <= exact payout, every case.
- 6 property tests x 512 random cases: round trips and trade sequences never profit, dust round trips pay 0, prices sum <= 1, maker loss <= b·ln(n), costs monotone.
- 5 integration tests of the compiled program with the real SPL Token program (litesvm).
- Worst case compute: 435k CU (8 outcomes, sell); clients request 600k.

## Instructions
create_market, buy (with max cost), sell (with min payout), resolve (creator), redeem. Slippage limits on buy and sell; state is a market PDA, a vault PDA (SPL token account) and per-trader position PDAs.

## Prior work
The LMSR design and math are ported from our own open-source project github.com/halfabillion/lmsr-fe (Apache-2.0). lmsr-sol is also Apache-2.0.
```

**Built with** (no dedicated field on the form; include in the description if needed)
> Rust, solana-program 2.3 (native, no Anchor), SPL Token, uint (256-bit mul_div), proptest, litesvm, cargo build-sbf, TypeScript (@solana/web3.js smoke script), Solana devnet.

---

## 2. Form step "Links"

| Field | Value | State |
|---|---|---|
| GitHub Repository | `https://github.com/halfabillion/lmsr-sol` (intended name) | **[BLANK]** until Ali/MMG approve making the repo public and it's pushed. Repo is private and local-only right now (no remote). |
| Demo URL | `https://explorer.solana.com/address/7uKjZAG4m5vFd77rWD1kasKV2YwQrzSaHeeFM6a7NWsz?cluster=devnet` | Ready (public devnet explorer page). Satisfies "at least one link". |
| Pitch Video URL | the hosted copy of `submit/demo/lmsr-sol-devnet-demo.mp4` | **[BLANK]** until it's uploaded (YouTube/Loom, unlisted) with Ali's approval. The form takes a URL, not a file. |
| Technical Video URL | (optional) | **[BLANK]**. Not made; optional. Needs Ali to record if wanted. |

Devnet transactions (all finalized; for the description or reviewer notes):

| Step | Explorer link |
|---|---|
| Program | https://explorer.solana.com/address/7uKjZAG4m5vFd77rWD1kasKV2YwQrzSaHeeFM6a7NWsz?cluster=devnet |
| Deploy (slot 524,398,076) | https://explorer.solana.com/tx/3wAGJFchQSmug7ugCqCys44jdwpKJhWcdMpZrGeH8Y4K1Yh8B8udUT7G9pEuVFRARiragZE6W9g7hsq9HSa4YCM5?cluster=devnet |
| create_market | https://explorer.solana.com/tx/24FAHPzpn4sgLUEpZDaxZShpyEB6GhK3RTytniS4SazWoJ6Saf2NFoD8MjMqKGdD8zzeRMktKBEptuUEtWX5AEEV?cluster=devnet |
| buy (40 sh, paid 15,181,215) | https://explorer.solana.com/tx/4RmDoD7shYvQkhrv728M3QCWxcKfX9NwSG4AKoraYxw4KWDizh72kLHQY6kGrY6EWp2TDUEZeUaksNf6C3M4U4p9?cluster=devnet |
| sell (40 sh, returned 15,181,214) | https://explorer.solana.com/tx/p9SW8gHcTA9U2fg37r2NiAZHqwXQawvqe2B164jeNrn1VfDSZp4NK5mfsfZUDRYX6t4zFDDUTuSqEqacsNZ1XWY?cluster=devnet |
| buy, outcome 2 (25 sh, 9,045,765) | https://explorer.solana.com/tx/5tkAjQ2MksxyzFs7Sou8LJBLC1oYqjDSLf8zCNgqEL9i9Cv7BiRZpZZQdjZQnyJqQUkBiCXdMNqxwD3HygwRHYhQ?cluster=devnet |
| resolve (winner outcome 2) | https://explorer.solana.com/tx/2LqEGVmmzLxiR2H4WcckgD6htZUsUaSD8xyMtAo4vPK2jKGDuRC3tRvhSTzHtuHaEQUgQ8DrhPgTjUAcEvgcTR6n?cluster=devnet |
| redeem (paid 25,000,000) | https://explorer.solana.com/tx/2PBufyDh7yvufq9ChRkbFWjfVERs7ueAp1ycLqYTTL4iE9pzNxnmda3ZyjNbRfKHm24j488PvLormTrAvcgN4iJ7?cluster=devnet |

Market `5paPuX85QgseyNnsm3i8QwSi7gZicQtdHriga19W3zm4`, vault `5rS8YbVTmkZjWJPjJNAJLpKFwQhPPVPSQB2MzkVaAWuJ`, test mint `7MXxsfU3KcRijAW3r551c54AbXMMy8aLwQ3XQ4yUBuAC`.

---

## 3. Demo video notes

- File: `submit/demo/lmsr-sol-devnet-demo.mp4`: 2:38 (158 s), 1920×1080, H.264 High, yuv420p, 30 fps, silent (no audio track), captions burned in. About 8.5 MB.
- Rebuild: `python3 submit/demo/render_demo.py` (reads `docs/SMOKE_devnet.json` plus the explorer stills in `submit/demo-stills/`).
- Content: title → pricing/rounding → program account → deploy → create_market → buy → sell → round-trip proof card (15,181,215 − 15,181,214 = 1) → buy outcome 2 → resolve → redeem (25,000,000) → tests → links. Includes the lmsr-fe Apache-2.0 disclosure caption.
- Fits the "pitch video (3 min max)" slot. Not a 5-minute technical walkthrough.
- Hosting: **[BLANK]**. Needs approval to upload (unlisted YouTube or Loom), then paste the URL into "Pitch Video URL".

---

## 4. Form step "Team"

| Item | Value | State |
|---|---|---|
| Team lead (submitter) | Ali Ghaedi | The account that submits is the lead. |
| GitHub | halfabillion | |
| Email | gh.ali7755@gmail.com | |
| X | @half_abillion | |
| Additional members | none | The form invites by site username plus role. No other members. |
| Site username for Ali | **[BLANK]**: confirm the username on the hackathons.solana.com profile before submitting. | Unverified |

---

## 5. Form step "Bounty Tracks"

- Main track only. **Do not select PREDICT pm-AMM** (skipped per instructions). Up to 3 sponsor tracks are allowed; none selected.

---

## 6. Payout

- Form text: *"Prize money will be sent to your linked wallet."* There's no free-text payout field, so the platform sends to the Solana wallet linked to the submitting account.
- Linked wallet (box Phantom, Solana): `HKJhkAZdkdNAmET7JoLHyTmDioka3CGYdajqaM31Wdgd`.
- Ali's preferred destination, Phantom **Base** `0xd9784f8122af988E51b3183F725b0b53BCb8278A`, **can't be entered**: the platform forces the linked Solana wallet. If there's a prize, forward or bridge from the linked wallet to the Base address after receipt, or follow whatever KYC/payout process the Solana Foundation runs when it contacts winners.

---

## 7. Eligibility

- Built for this hackathon: the on-chain program, tests and devnet deploy are all new work. The LMSR design is ported from our own prior open-source repo lmsr-fe (Apache-2.0), and that's disclosed.
- Live demo on devnet (allowed: "devnet or mainnet").
- Repo public: **pending**. If the rules published at launch require a public repo, it has to be made public before 5 Dec 21:00 UK, and that needs Ali/MMG approval.
- Rules text is `null` on the listing today. **Recheck eligibility, judging criteria and any disclosure rules when the hackathon launches.**

---

## 8. AI-disclosure answer

- **No AI-disclosure field or rule found** on the submit form or listing (checked 21 Nov 2026). Leave out unless one appears at launch. If one is added, answer it truthfully then.

## 9. Interested in publishing

- **Yes.** No such field exists on the current form. If it's added, answer Yes.

---

## Before anyone submits (checklist)

1. MMG rechecks the rules once `rules`/launch details are published.
2. Approval to make `halfabillion/lmsr-sol` public (and to push it). Then fill in GitHub Repository.
3. Approval to upload the demo (unlisted). Then fill in Pitch Video URL.
4. Confirm Ali's hackathons.solana.com username and linked wallet.
5. Ali submits personally, or explicitly authorises submission. Use Save Draft first.
