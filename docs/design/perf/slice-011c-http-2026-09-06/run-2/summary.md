Protocol `slice-011c-authenticated-http-v1`; fixture people=50,000 inquiries=46,064 contact facts=52,306 corrections=2,906 inbound=2,843; clock 2026-09-06T12:00:00Z; source hash `edf1a06a0790…`; build hash `c07e21b0fe7e…`.
Series: 15; late capture reconciliations: 0; sentinel safety: {'forbidden_fixture_sentinels_absent': True, 'required_safe_telemetry_fields_present': True}.

| Case | Series | Arm | Attempts exp/rec/complete | Source evals | c1 p95 / cap | c10 p95 / cap | c20 p95 / cap | Whole-source p95 / max | Enum max | Auth acq p95 / max | Feed acq p95 / max | Feed headroom |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| concentrated_zero_sources | paired_original_zero_source | frozen_original | 68/68/48 **NO** | 0 | 703 / 1,250 | 3,331 / 2,500 **NO** | 3,328 / 4,500 | – / – | – | 2 / 2 | 2,002 / 2,002 | – |
| typical_zero_sources | paired_original_zero_source | frozen_original | 35/35/35 | 0 | 524 / 1,250 | 844 / 2,500 | 1,464 / 4,500 | – / – | – | 2 / 2 | 857 / 896 | 1,104 |
| partial_builtins_zero_sources | paired_original_zero_source | frozen_original | 35/35/35 | 0 | 409 / 1,250 | 671 / 2,500 | 1,129 / 4,500 | – / – | – | 2 / 2 | 689 / 707 | 1,293 |
| empty_builtins_zero_sources | paired_original_zero_source | frozen_original | 35/35/35 | 0 | 377 / 1,250 | 695 / 2,500 | 1,524 / 4,500 | – / – | – | 1 / 1 | 920 / 948 | 1,052 |
| concentrated_zero_sources | final_matrix | final | 68/68/68 | 0 | 223 / 1,250 | 441 / 2,500 | 782 / 4,500 | – / – | 18 | 1 / 1 | 482 / 500 | 1,500 |
| concentrated_one_dense_source | final_matrix | final | 68/68/68 | 68 | 322 / 1,250 | 662 / 2,500 | 1,319 / 4,500 | 309 / 335 | 20 | 2 / 2 | 727 / 899 | 1,101 |
| concentrated_one_absence_source | final_matrix | final | 68/68/68 | 68 | 302 / 1,250 | 595 / 2,500 | 1,072 / 4,500 | 188 / 201 | 15 | 2 / 2 | 595 / 611 | 1,389 |
| concentrated_five_overlapping_sources | final_matrix | final | 68/68/68 | 340 | 680 / 1,250 | 1,332 / 2,500 | 2,710 / 4,500 | 286 / 358 | 25 | 2 / 2 | 1,424 / 1,586 | 414 |
| typical_zero_sources | final_matrix | final | 35/35/35 | 0 | 28 / 1,250 | 60 / 2,500 | 103 / 4,500 | – / – | 15 | 1 / 1 | 57 / 58 | 1,942 |
| typical_five_overlapping_sources | final_matrix | final | 35/35/35 | 175 | 41 / 1,250 | 96 / 2,500 | 167 / 4,500 | 13 / 20 | 9 | 1 / 1 | 96 / 100 | 1,900 |
| partial_builtins_zero_sources | final_matrix | final | 35/35/35 | 0 | 10 / 1,250 | 21 / 2,500 | 43 / 4,500 | – / – | 9 | 2 / 2 | 21 / 23 | 1,977 |
| partial_builtins_five_overlapping_sources | final_matrix | final | 35/35/35 | 175 | 546 / 1,250 | 1,015 / 2,500 | 2,121 / 4,500 | 321 / 391 | 23 | 1 / 1 | 1,178 / 1,203 | 797 |
| empty_builtins_zero_sources | final_matrix | final | 35/35/35 | 0 | 6 / 1,250 | 11 / 2,500 | 19 / 4,500 | – / – | 4 | 1 / 1 | 10 / 14 | 1,986 |
| empty_builtins_five_overlapping_sources | final_matrix | final | 35/35/35 | 175 | 505 / 1,250 | 1,020 / 2,500 | 1,910 / 4,500 | 275 / 347 | 13 | 1 / 2 | 1,033 / 1,097 | 903 |
| concentrated_five_overlapping_sources | independent_concentrated_repeat | final | 40/40/40 | 200 | – | – | 2,725 / 4,500 | 287 / 341 | 19 | 1 / 1 | 1,488 / 1,557 | 443 |

| Paired zero-source case | Comparison | Payload parity | Hash parity | c1 orig→final (allowed) | c10 orig→final (allowed) | c20 orig→final (allowed) |
|---|---|---|---|---|---|---|
| concentrated_zero_sources | yes | **NO** | **NO** | 703→223 (773) | 3,331→441 (3,664) | 3,328→782 (3,661) |
| typical_zero_sources | yes | yes | yes | 524→28 (576) | 844→60 (928) | 1,464→103 (1,610) |
| partial_builtins_zero_sources | yes | yes | yes | 409→10 (450) | 671→21 (738) | 1,129→43 (1,242) |
| empty_builtins_zero_sources | yes | yes | yes | 377→6 (415) | 695→11 (764) | 1,524→19 (1,676) |

Measured attempts: final 522, original 173; warm-up attempts 900; final whole-source evaluations 1201.
Series passing the harness's normal-completion gate: 14 of 15.
